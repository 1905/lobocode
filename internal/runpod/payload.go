package runpod

import (
	"github.com/1905/lobocode/internal/bootstrap"
	"github.com/1905/lobocode/internal/provider"
)

const (
	PodName = provider.Name
	GPUType = "NVIDIA GeForce RTX 5090"
)

// sshPrefix starts sshd for `lobo up --ssh` debugging. Not used in normal runs.
const sshPrefix = `apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq openssh-server >/dev/null
mkdir -p /root/.ssh /run/sshd && echo "$PUBLIC_KEY" > /root/.ssh/authorized_keys && chmod 600 /root/.ssh/authorized_keys && /usr/sbin/sshd
`

// BuildCreatePayload is the POST /pods body for one cloud and network tier. It never contains R2 or account keys.
func BuildCreatePayload(o provider.CreateOpts, cloud string, minDownloadMbps float64) map[string]any {
	if cloud == "" {
		cloud = "SECURE"
	}
	ports, start := []string{}, bootstrap.Script("runpod")
	env := bootstrap.Env(o, "runpod")
	if o.SSHPubKey != "" {
		ports, start = []string{"22/tcp"}, sshPrefix+start
		env["PUBLIC_KEY"] = o.SSHPubKey
	}
	payload := map[string]any{
		"name":              PodName,
		"imageName":         o.Image,
		"gpuTypeIds":        []string{GPUType},
		"gpuCount":          1,
		"cloudType":         cloud,
		"containerDiskInGb": 60,
		"volumeInGb":        0,
		"ports":             ports,
		"dockerEntrypoint":  []string{"bash", "-c"},
		"dockerStartCmd":    []string{start},
		"env":               env,
	}
	if minDownloadMbps > 0 {
		payload["minDownloadMbps"] = minDownloadMbps
	}
	return payload
}
