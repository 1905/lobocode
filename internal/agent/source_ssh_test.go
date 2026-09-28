package agent

import (
	"bytes"
	"context"
	"crypto/ecdsa"
	"crypto/ed25519"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"fmt"
	"net"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync/atomic"
	"testing"

	"golang.org/x/crypto/ssh"
)

// fakeFeesh is an in-process SSH server with the model server's forced-command contract: "<file> <offset>".
// The first session drops halfway, to exercise resume.
func fakeFeesh(t *testing.T, data []byte, clientPub ssh.PublicKey) (addr string, hostKey ssh.PublicKey, sessions *atomic.Int32) {
	_, hpriv, _ := ed25519.GenerateKey(rand.Reader)
	hs, _ := ssh.NewSignerFromKey(hpriv)
	// Like the model server: also offer an ECDSA host key, which a default client may pick over the pinned ed25519 one.
	epriv, _ := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	es, _ := ssh.NewSignerFromKey(epriv)
	cfg := &ssh.ServerConfig{PublicKeyCallback: func(_ ssh.ConnMetadata, k ssh.PublicKey) (*ssh.Permissions, error) {
		if bytes.Equal(k.Marshal(), clientPub.Marshal()) {
			return nil, nil
		}
		return nil, fmt.Errorf("denied")
	}}
	cfg.AddHostKey(es)
	cfg.AddHostKey(hs)
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = ln.Close() })
	sessions = &atomic.Int32{}
	go func() {
		for {
			c, err := ln.Accept()
			if err != nil {
				return
			}
			go func(c net.Conn) {
				_, chans, reqs, err := ssh.NewServerConn(c, cfg)
				if err != nil {
					return
				}
				go ssh.DiscardRequests(reqs)
				for nc := range chans {
					ch, creqs, _ := nc.Accept()
					for req := range creqs {
						if req.Type != "exec" {
							_ = req.Reply(false, nil)
							continue
						}
						_ = req.Reply(true, nil)
						n := sessions.Add(1)
						cmd := string(req.Payload[4:])
						f := strings.Fields(cmd)
						off, _ := strconv.ParseInt(f[1], 10, 64)
						chunk := data[off:]
						if len(f) > 2 {
							l, _ := strconv.ParseInt(f[2], 10, 64)
							chunk = chunk[:l]
						}
						if n == 1 {
							chunk = chunk[:len(chunk)/2]
							_, _ = ch.Write(chunk)
							_ = c.Close() // drop the connection mid-stream
							return
						}
						_, _ = ch.Write(chunk)
						st := make([]byte, 4)
						binary.BigEndian.PutUint32(st, 0)
						_, _ = ch.SendRequest("exit-status", false, st)
						_ = ch.Close()
					}
				}
			}(c)
		}
	}()
	return ln.Addr().String(), hs.PublicKey(), sessions
}

func TestSSHSourceResume(t *testing.T) {
	data := make([]byte, 1<<20)
	_, _ = rand.Read(data)
	sum := sha256.Sum256(data)
	_, cpriv, _ := ed25519.GenerateKey(rand.Reader)
	signer, _ := ssh.NewSignerFromKey(cpriv)
	addr, hk, sessions := fakeFeesh(t, data, signer.PublicKey())
	dst := filepath.Join(t.TempDir(), "m")
	src := SSHSource{Addr: addr, User: "lobo", File: "m.gguf", Size: int64(len(data)), Signer: signer, HostKey: hk}
	if err := Download(context.Background(), src, dst, hex.EncodeToString(sum[:]), nil); err != nil {
		t.Fatal(err)
	}
	got, _ := os.ReadFile(dst)
	if !bytes.Equal(got, data) || sessions.Load() != 2 {
		t.Fatalf("equal %v sessions %d", bytes.Equal(got, data), sessions.Load())
	}
}

func TestSSHSourceRejectsWrongHostKey(t *testing.T) {
	_, cpriv, _ := ed25519.GenerateKey(rand.Reader)
	signer, _ := ssh.NewSignerFromKey(cpriv)
	addr, _, _ := fakeFeesh(t, []byte("x"), signer.PublicKey())
	_, other, _ := ed25519.GenerateKey(rand.Reader)
	otherSigner, _ := ssh.NewSignerFromKey(other)
	src := SSHSource{Addr: addr, User: "lobo", File: "m", Size: 1, Signer: signer, HostKey: otherSigner.PublicKey()}
	_, _, err := src.Open(context.Background(), 0, -1)
	if err == nil || !strings.Contains(err.Error(), "host key") {
		t.Fatal(err)
	}
	if _, ok := err.(permanentErr); !ok {
		t.Fatal("host key failure must be permanent (no retries → no fail2ban ban)")
	}
}
