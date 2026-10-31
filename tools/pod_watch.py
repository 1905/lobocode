"""Poll a bench pod every minute and print a line only when GPU money is being wasted.

Usage: python3 pod_watch.py HOST PORT LOG [PID]
Prints:
  IDLE   GPU util 0% and no log write for 60s+, 2+ checks in a row
  STALL  LOG not written for 3+ minutes
  ORPHAN the sweep process PID is gone but the pod still answers SSH
  STATUS one line every 5 checks
"""
import os
import sys
import time

import spec_bench as sb

host, port, log = sys.argv[1], int(sys.argv[2]), sys.argv[3]
pid = int(sys.argv[4]) if len(sys.argv) > 4 else None
pod = sb.Pod(host, port)


def alive():
    if pid is None:
        return True
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False


idle = 0
n = 0
while True:
    n += 1
    now = time.strftime("%H:%M:%S")
    try:
        out = pod.sh("nvidia-smi --query-gpu=utilization.gpu,memory.used --format=csv,noheader,nounits; "
                     "pgrep -c llama-server || true", timeout=30)
        lines = out.splitlines()
        util, mem = [int(x) for x in lines[0].split(",")]
        servers = lines[1].strip() if len(lines) > 1 else "?"
    except Exception as e:  # pod gone or SSH flaky
        print(f"{now} SSH-FAIL {str(e)[:120]} sweep_alive={alive()}", flush=True)
        if not alive():
            sys.exit(0)
        time.sleep(60)
        continue
    age = time.time() - os.path.getmtime(log) if os.path.exists(log) else -1
    idle = idle + 1 if util == 0 and age > 60 else 0
    if idle >= 2:
        print(f"{now} IDLE gpu=0% x{idle} mem={mem}MiB llama-server={servers} log_age={age:.0f}s", flush=True)
    if age > 180:
        print(f"{now} STALL log not written for {age:.0f}s gpu={util}%", flush=True)
    if not alive():
        print(f"{now} ORPHAN sweep pid {pid} exited, pod still up gpu={util}%", flush=True)
    elif n % 5 == 0:
        print(f"{now} STATUS gpu={util}% mem={mem}MiB llama-server={servers} log_age={age:.0f}s", flush=True)
    time.sleep(60)
