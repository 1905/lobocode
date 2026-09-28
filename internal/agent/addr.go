// Package agent is the pod side: boot stages, supervision, watchdog, /api.
package agent

// One routing mode (P1 verified tunnel path rules): /api/* → agent, rest → llama-server.
const (
	LlamaAddr = "127.0.0.1:8080"
	AgentAddr = "127.0.0.1:8081"
)
