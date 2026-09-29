package main

import (
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"

	"github.com/joho/godotenv"
	"github.com/spf13/cobra"

	"github.com/1905/lobocode/internal/config"
	"github.com/1905/lobocode/internal/model"
)

const opencodeOut = "opencode.lobo.json" // gitignored: holds the API key

func genKeyCmd() *cobra.Command {
	var rotate bool
	c := &cobra.Command{
		Use:   "gen-api-key",
		Short: "Create LOBO_API_KEY in the config once (kept across ups) and write opencode.lobo.json with it",
		RunE: func(*cobra.Command, []string) error {
			env, err := godotenv.Read(cfgPath)
			if err != nil {
				return fmt.Errorf("read %s: %w", cfgPath, err)
			}
			key := env["LOBO_API_KEY"]
			switch {
			case key == "" || rotate:
				b := make([]byte, 24)
				if _, err := rand.Read(b); err != nil {
					return err
				}
				key = "sk-" + hex.EncodeToString(b)
				if err := config.SetEnvValue(cfgPath, "LOBO_API_KEY", key); err != nil {
					return err
				}
				log.Info().Msg("new LOBO_API_KEY written to the config (a running pod keeps the old key until the next `lobo up`)")
			default:
				log.Info().Msg("keeping existing LOBO_API_KEY (use --rotate for a new one)")
			}
			domain := env["LOBO_DOMAIN"]
			if domain == "" {
				log.Info().Msg("LOBO_DOMAIN is empty: writing only the lobo-local provider (this Mac)")
			}
			port := config.Laptop{LocalPort: env["LOBO_LOCAL_PORT"]}.Port()
			if err := writeOpencode(opencodeOut, domain, key, port); err != nil {
				return err
			}
			abs, _ := filepath.Abs(opencodeOut)
			fmt.Printf("wrote %s\n", abs)
			return nil
		},
	}
	c.Flags().BoolVar(&rotate, "rotate", false, "replace the existing key")
	return c
}

// writeOpencode writes an OpenCode config with the key inline (no env var needed): provider lobo (the pod at
// domain, skipped when domain is empty) and lobo-local (this Mac on 127.0.0.1:port), same models.
func writeOpencode(path, domain, key string, port int) error {
	// One entry: the pod serves one model at a time (q8 default). Short names: the picker truncates.
	m, _ := model.Get("q8")
	models := map[string]any{
		m.Alias: map[string]any{
			"name":  "Qwen3.5-27B Q8",
			"limit": map[string]int{"context": 65536, "output": 8192}, // must match the pod ctx (release default)
		},
	}
	// A dedicated agent keeps the prompt small for lobo only (measured 2026-09-25, OpenCode 1.18):
	// default 42,949 tokens → MCP off 25,226 → also skill/webfetch/todo/task off 15,008.
	// MCP server tools (blender_*, pencil_*) come from the user's global config; the globs are harmless if absent.
	prov := func(name, base string) map[string]any {
		return map[string]any{
			"npm":     "@ai-sdk/openai-compatible",
			"name":    name,
			"options": map[string]string{"baseURL": base, "apiKey": key},
			"models":  models,
		}
	}
	providers := map[string]any{"lobo-local": prov("Lobo (this Mac)", fmt.Sprintf("http://127.0.0.1:%d/v1", port))}
	agentModel := "lobo-local/" + m.Alias
	if domain != "" {
		providers["lobo"] = prov("Lobo", "https://"+domain+"/v1")
		agentModel = "lobo/" + m.Alias
	}
	agent := map[string]any{
		"lobo": map[string]any{
			"description": "Lean agent for the lobo pod (Qwen3.5-27B): no MCP, no skills, core coding tools only",
			"mode":        "primary",
			"model":       agentModel,
			"tools": map[string]bool{
				"blender_*": false, "pencil_*": false, "skill": false,
				"webfetch": false, "todowrite": false, "todoread": false, "task": false,
			},
		},
	}
	cfg := map[string]any{
		"$schema":  "https://opencode.ai/config.json",
		"agent":    agent,
		"provider": providers,
	}
	b, err := json.MarshalIndent(cfg, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, append(b, '\n'), 0o600)
}
