// corefixtures captures deterministic compatibility fixtures from the Go implementation.
// It never reads the user's config or calls a provider. Removed at Rust cutover.
package main

import (
	"encoding/json"
	"fmt"
	"github.com/1905/lobocode/internal/bootstrap"
	"github.com/1905/lobocode/internal/config"
	"github.com/1905/lobocode/internal/local"
	"github.com/1905/lobocode/internal/provider"
	"github.com/1905/lobocode/internal/runpod"
	"github.com/1905/lobocode/internal/vast"
	"github.com/joho/godotenv"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"syscall"
	"time"
)

func must(err error) {
	if err != nil {
		panic(err)
	}
}
func put(path string, b []byte) {
	must(os.MkdirAll(filepath.Dir(path), 0700))
	must(os.WriteFile(path, b, 0600))
}
func jsonFile(path string, v any) {
	b, e := json.MarshalIndent(v, "", "  ")
	must(e)
	put(path, append(b, '\n'))
}
func read(path string) []byte { b, e := os.ReadFile(path); must(e); return b }
func main() {
	if len(os.Args) < 3 {
		panic("usage: corefixtures dump DIR | holdlock PATH MS | trylock PATH | readstate XDG")
	}
	switch os.Args[1] {
	case "dump":
		dump(os.Args[2])
	case "holdlock", "trylock":
		f, e := os.OpenFile(os.Args[2], os.O_CREATE|os.O_RDWR, 0600)
		must(e)
		defer f.Close()
		if os.Args[1] == "trylock" {
			e = syscall.Flock(int(f.Fd()), syscall.LOCK_EX|syscall.LOCK_NB)
			if e == syscall.EWOULDBLOCK {
				fmt.Println("busy")
				return
			}
			must(e)
			fmt.Println("free")
			return
		}
		must(syscall.Flock(int(f.Fd()), syscall.LOCK_EX))
		fmt.Println("locked")
		ms, e := strconv.Atoi(os.Args[3])
		must(e)
		time.Sleep(time.Duration(ms) * time.Millisecond)
	case "readstate":
		must(os.Setenv("XDG_STATE_HOME", os.Args[2]))
		s, ok, e := local.ReadState()
		must(e)
		must(json.NewEncoder(os.Stdout).Encode(map[string]any{"ok": ok, "pid": s.PID, "boot_id": s.BootID}))
	default:
		panic("unknown mode")
	}
}
func dump(out string) {
	tmp, e := os.MkdirTemp("", "lobo-core-fixtures-")
	must(e)
	defer os.RemoveAll(tmp)
	dotenv := map[string]string{
		"plain": "A=one\nB=two\n", "export_prefix": "export A=one\n export B=two\n",
		"comments_and_blank": "# note\n\n A=one\n\t# end\n", "inline_comment": "A=one # note\nB=two#literal\n",
		"double_quoted_escapes": `A="line\nnext\rquote\" slash\\ dollar\$"` + "\n",
		"single_quoted":         "A='x $HOME \\n'\n", "expand_earlier_key": "A=x\nB=$A-${A}\n",
		"expand_os_home_is_empty": "H=$HOME\n", "escaped_dollar": `A="\$HOME"` + "\nB=\\$HOME\n",
		"yaml_colon": "K: v\n", "crlf": "A=one\r\nB=two\r\n", "no_trailing_newline": "A=one",
		"empty_value": "A=\nB=\"\"\n", "unterminated_quote": "A=\"unfinished\n", "bad_key_char": "BAD-KEY=value\n",
	}
	for name, src := range dotenv {
		path := filepath.Join(tmp, "input.env")
		put(path, []byte(src))
		v, e := godotenv.Read(path)
		jsonPath := filepath.Join(out, "dotenv", name+".json")
		put(filepath.Join(out, "dotenv", name+".env"), []byte(src))
		if e != nil {
			jsonFile(jsonPath, map[string]string{"error": e.Error()})
		} else {
			jsonFile(jsonPath, v)
		}
	}
	all := map[string]string{"ZZ_EXTRA": "z", "AA_EXTRA": "a"}
	for _, g := range config.Layout {
		for _, key := range g.Keys {
			all[key] = "value"
		}
	}
	type saveCase struct {
		name   string
		before *string
		set    map[string]string
	}
	text := func(s string) *string { return &s }
	saves := []saveCase{
		{"new_file", nil, map[string]string{"VASTAI_API_KEY": "vk", "LOBO_PROVIDER": "vast", "R2_ENDPOINT": "https://e"}},
		{"new_file_all_keys", nil, all},
		{"keeps_hand_edits", text("# my notes\nRUNPOD_API_KEY=old\nCUSTOM_THING=1\nexport LOBO_DOMAIN=a.b\n"), map[string]string{"RUNPOD_API_KEY": "new", "LOBO_DOMAIN": "", "LOBO_MODEL_SSH_HOSTKEY": "ssh-ed25519 AAAA#x", "LOBO_MIN_MBPS": "150"}},
		{"set_env_value.step1", text("# comment\nA=1\nLOBO_API_KEY=old\nB=2\n"), map[string]string{"LOBO_API_KEY": "new"}},
		{"set_env_value.step2", text("# comment\nA=1\nLOBO_API_KEY=new\nB=2\n"), map[string]string{"C": "3"}},
		{"special_values", nil, map[string]string{"A": "with space", "B": "hash#x", "C": "quote\"slash\\", "D": "$HOME", "E": "line\nnext", "F": "a\tb", "G": "x\ry", "H": "it's"}},
		{"export_and_spaces", text(" export  LOBO_DOMAIN = a\n"), map[string]string{"LOBO_DOMAIN": "b"}},
		{"duplicate_key", text("A=old\n# note\nA=second\n"), map[string]string{"A": "new"}},
		{"empty_removes", text("A=one\nB=two\n"), map[string]string{"A": ""}},
		{"commented_key_kept", text("#RUNPOD_API_KEY=x\n"), map[string]string{"RUNPOD_API_KEY": "new"}},
		{"crlf_file", text("# note\r\nA=one\r\nB=two\r\n"), map[string]string{"A": "new"}},
		{"empty_file_gets_layout", text(""), map[string]string{"LOBO_MODEL": "q6"}},
		{"whitespace_file_is_existing", text("\n\n"), map[string]string{"LOBO_MODEL": "q6"}},
	}
	for _, c := range saves {
		path := filepath.Join(tmp, c.name+".env")
		base := filepath.Join(out, "config_save", c.name)
		if c.before != nil {
			put(path, []byte(*c.before))
			put(base+".before.env", []byte(*c.before))
		}
		jsonFile(base+".set.json", c.set)
		must(config.Save(path, c.set))
		put(base+".after.env", read(path))
	}
	for _, p := range []string{"runpod", "vast"} {
		put(filepath.Join(out, "bootstrap", "script_"+p+".sh"), []byte(bootstrap.Script(p)))
	}
	opts := provider.CreateOpts{Image: "img", ReleaseURL: "https://b/r.zip", ReleaseSHA256: "abc", ModelURL: "https://b/m.gguf", LoboAPIKey: "sk", CFTunnelToken: "tok", Model: "q8", Ctx: 8192, IdleMin: 30, ExpiresAt: time.Date(2026, 9, 23, 22, 0, 0, 0, time.UTC)}
	jsonFile(filepath.Join(out, "runpod", "payload_community.json"), runpod.BuildCreatePayload(opts, "COMMUNITY", 0))
	ssh := opts
	ssh.SSHPubKey = "ssh-ed25519 AAAATEST"
	jsonFile(filepath.Join(out, "runpod", "payload_secure_5000_ssh.json"), runpod.BuildCreatePayload(ssh, "SECURE", 5000))
	full := opts
	full.BootID = "boot-fixed"
	full.ModelSSHKey = "base64-fixed"
	full.ModelHostKey = "ssh-ed25519 AAAAPIN"
	full.ModelFallback = "https://fallback/m"
	full.DLConns = 8
	full.MinMBps = 100
	jsonFile(filepath.Join(out, "bootstrap", "env_full.json"), bootstrap.Env(full, "runpod"))
	full.ReleaseURL = ""
	full.ReleaseSHA256 = ""
	jsonFile(filepath.Join(out, "bootstrap", "env_baked.json"), bootstrap.Env(full, "runpod"))
	vo := opts
	vo.Image = "ghcr.io/ggml-org/llama.cpp:server-cuda-b11118"
	vo.ReleaseURL = "r"
	vo.ReleaseSHA256 = "s"
	vo.ModelURL = "m"
	vo.Ctx = 65536
	jsonFile(filepath.Join(out, "vast", "create_body.json"), vast.CreateBody(vo))
	vo.ReleaseURL = ""
	vo.ReleaseSHA256 = ""
	jsonFile(filepath.Join(out, "vast", "create_body_baked.json"), vast.CreateBody(vo))
	put(filepath.Join(out, "runpod", "pod.json"), read("internal/runpod/testdata/pod.json"))
	bin := filepath.Join(tmp, "lobo")
	build := exec.Command("go", "build", "-o", bin, "./cmd/lobo")
	if b, e := build.CombinedOutput(); e != nil {
		panic(fmt.Sprintf("build fixture CLI: %v: %s", e, b))
	}
	for name, extra := range map[string]string{"cloud": "LOBO_DOMAIN=lobo.example.com\n", "local_only": "LOBO_LOCAL_PORT=9000\n"} {
		dir := filepath.Join(tmp, name)
		cfg := filepath.Join(dir, "config.env")
		put(cfg, []byte("LOBO_API_KEY=sk-fixed\n"+extra))
		cmd := exec.Command(bin, "--config", cfg, "gen-api-key")
		cmd.Dir = dir
		if b, e := cmd.CombinedOutput(); e != nil {
			panic(fmt.Sprintf("genkey fixture: %v: %s", e, b))
		}
		put(filepath.Join(out, "opencode", name+".json"), read(filepath.Join(dir, "opencode.lobo.json")))
	}
}
