package agent

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"strconv"
	"time"

	"golang.org/x/crypto/ssh"
)

// Source streams the model from a byte offset. length < 0 = to the end.
// total is the full file size when the source knows it, else -1.
type Source interface {
	Open(ctx context.Context, offset, length int64) (body io.ReadCloser, total int64, err error)
	String() string
}

// permanentErr stops resuming: the source will not get better.
type permanentErr struct{ error }

// HTTPSource: plain GET, resumed with Range.
type HTTPSource struct{ URL string }

// String is scheme://host only: errors built from it land on the public /api/status, and the path/query
// of a presigned R2 or the model server URL is the credential.
func (s HTTPSource) String() string { return RedactURL(s.URL) }

// RedactURL keeps scheme://host of a URL.
func RedactURL(raw string) string {
	u, err := url.Parse(raw)
	if err != nil || u.Host == "" {
		return "?"
	}
	return u.Scheme + "://" + u.Host
}

func (s HTTPSource) Open(ctx context.Context, offset, length int64) (io.ReadCloser, int64, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, s.URL, nil)
	if err != nil {
		return nil, 0, permanentErr{err}
	}
	ranged := offset > 0 || length >= 0
	if ranged {
		end := ""
		if length >= 0 {
			end = strconv.FormatInt(offset+length-1, 10)
		}
		req.Header.Set("Range", "bytes="+strconv.FormatInt(offset, 10)+"-"+end)
	}
	// The read-stall timer in fetchRanges starts only once Open returns. A server that accepts the
	// request and never sends headers would hang this stream until the boot timeout, so bound the wait.
	rctx, cancel := context.WithCancel(ctx)
	headers := time.AfterFunc(StallTimeout, cancel)
	resp, err := http.DefaultClient.Do(req.WithContext(rctx))
	headers.Stop()
	if err != nil {
		cancel()
		if ctx.Err() == nil && rctx.Err() != nil {
			err = fmt.Errorf("no response headers after %s", StallTimeout)
		}
		var ue *url.Error
		if errors.As(err, &ue) { // url.Error prints the full URL, secret path and query included
			err = fmt.Errorf("%s %s: %w", ue.Op, s, ue.Err)
		}
		return nil, 0, err
	}
	body := cancelBody{resp.Body, cancel}
	switch {
	case !ranged && resp.StatusCode == http.StatusOK:
		return body, resp.ContentLength, nil
	case ranged && resp.StatusCode == http.StatusPartialContent:
		return body, -1, nil
	}
	_ = body.Close()
	switch {
	case ranged && resp.StatusCode == http.StatusOK:
		return nil, 0, permanentErr{fmt.Errorf("server ignored Range at offset %d, can't resume", offset)}
	case resp.StatusCode >= 500 || resp.StatusCode == http.StatusTooManyRequests:
		return nil, 0, fmt.Errorf("HTTP %d", resp.StatusCode)
	}
	return nil, 0, permanentErr{fmt.Errorf("download %s: HTTP %d", s, resp.StatusCode)}
}

// cancelBody releases the request context when the body is closed.
type cancelBody struct {
	io.ReadCloser
	cancel context.CancelFunc
}

func (b cancelBody) Close() error {
	err := b.ReadCloser.Close()
	b.cancel()
	return err
}

// SSHSource streams from the model server. The server runs a forced command: "<file> <offset>" → bytes from offset.
type SSHSource struct {
	Addr    string // host:port
	User    string
	File    string
	Size    int64
	Signer  ssh.Signer
	HostKey ssh.PublicKey // pinned; any other host key is refused
}

func (s SSHSource) String() string { return fmt.Sprintf("ssh://%s@%s/%s", s.User, s.Addr, s.File) }

type sshBody struct {
	io.Reader
	sess   *ssh.Session
	client *ssh.Client
	stop   func() bool
}

func (b sshBody) Close() error {
	b.stop()
	_ = b.sess.Close()
	return b.client.Close()
}

func (s SSHSource) Open(ctx context.Context, offset, length int64) (io.ReadCloser, int64, error) {
	cfg := &ssh.ClientConfig{
		User:            s.User,
		Auth:            []ssh.AuthMethod{ssh.PublicKeys(s.Signer)},
		HostKeyCallback: ssh.FixedHostKey(s.HostKey),
		// Negotiate only the pinned key's type. Without this, the model server offered another host key type,
		// the pin failed, and 4 retries got the pod IP banned by fail2ban (2026-09-23).
		HostKeyAlgorithms: hostKeyAlgos(s.HostKey),
		Timeout:           20 * time.Second,
	}
	var d net.Dialer
	conn, err := d.DialContext(ctx, "tcp", s.Addr)
	if err != nil {
		return nil, 0, err
	}
	// ClientConfig.Timeout only covers ssh.Dial, not NewClientConn: a server that accepts TCP and never
	// speaks would hang the handshake (and this stream) until the boot timeout. Deadline cleared after Start.
	_ = conn.SetDeadline(time.Now().Add(StallTimeout))
	c, chans, reqs, err := ssh.NewClientConn(conn, s.Addr, cfg)
	if err != nil {
		_ = conn.Close()
		// Handshake/auth failures don't fix themselves, and each retry counts toward fail2ban.
		return nil, 0, permanentErr{fmt.Errorf("ssh %s: %w", s.Addr, err)}
	}
	client := ssh.NewClient(c, chans, reqs)
	sess, err := client.NewSession()
	if err != nil {
		_ = client.Close()
		return nil, 0, err
	}
	out, err := sess.StdoutPipe()
	if err != nil {
		_ = client.Close()
		return nil, 0, err
	}
	cmd := fmt.Sprintf("%s %d", s.File, offset)
	if length >= 0 {
		cmd += fmt.Sprintf(" %d", length)
	}
	if err := sess.Start(cmd); err != nil {
		_ = client.Close()
		return nil, 0, err
	}
	_ = conn.SetDeadline(time.Time{}) // from here the read-stall timer in fetchRanges guards the stream
	// Close the connection when ctx ends, so a stalled read returns. Close stops the watch (no goroutine per chunk).
	stop := context.AfterFunc(ctx, func() { _ = client.Close() })
	return sshBody{Reader: out, sess: sess, client: client, stop: stop}, s.Size, nil
}

func hostKeyAlgos(k ssh.PublicKey) []string {
	if k.Type() == ssh.KeyAlgoRSA {
		return []string{ssh.KeyAlgoRSASHA512, ssh.KeyAlgoRSASHA256, ssh.KeyAlgoRSA}
	}
	return []string{k.Type()}
}
