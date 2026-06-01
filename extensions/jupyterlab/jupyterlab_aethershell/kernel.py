"""AetherShell Jupyter kernel wrapper.

Proxies notebook cell execution to the AetherShell Agent API.
Install: pip install jupyterlab-aethershell
"""

import json
import urllib.request
from ipykernel.kernelbase import Kernel


class AetherShellKernel(Kernel):
    implementation = "AetherShell"
    implementation_version = "0.1.0"
    language = "aethershell"
    language_version = "1.3.1"
    language_info = {
        "name": "aethershell",
        "mimetype": "text/x-aethershell",
        "file_extension": ".ae",
    }
    banner = "AetherShell — one language, every platform, deterministic typed output"

    agent_api_url = "http://localhost:3002"

    def do_execute(self, code, silent, store_history=True, user_expressions=None, allow_stdin=False):
        if not silent:
            try:
                result = self._eval(code)
                output = json.dumps(result, indent=2) if isinstance(result, (dict, list)) else str(result)
                self.send_response(
                    self.iopub_socket,
                    "stream",
                    {"name": "stdout", "text": output + "\n"},
                )
            except Exception as e:
                self.send_response(
                    self.iopub_socket,
                    "stream",
                    {"name": "stderr", "text": f"Error: {e}\n"},
                )
                return {
                    "status": "error",
                    "ename": type(e).__name__,
                    "evalue": str(e),
                    "traceback": [],
                    "execution_count": self.execution_count,
                }

        return {
            "status": "ok",
            "execution_count": self.execution_count,
            "payload": [],
            "user_expressions": {},
        }

    def do_complete(self, code, cursor_pos):
        try:
            req = urllib.request.Request(
                f"{self.agent_api_url}/api/v1/builtins",
                headers={"Content-Type": "application/json"},
            )
            with urllib.request.urlopen(req, timeout=5) as resp:
                data = json.loads(resp.read())
                names = [b["name"] for b in data.get("builtins", [])]

                # Simple prefix matching
                token = code[:cursor_pos].split()[-1] if code[:cursor_pos].strip() else ""
                matches = [n for n in names if n.startswith(token)]

                return {
                    "status": "ok",
                    "matches": matches,
                    "cursor_start": cursor_pos - len(token),
                    "cursor_end": cursor_pos,
                    "metadata": {},
                }
        except Exception:
            return {
                "status": "ok",
                "matches": [],
                "cursor_start": cursor_pos,
                "cursor_end": cursor_pos,
                "metadata": {},
            }

    def _eval(self, code: str):
        payload = json.dumps({"code": code}).encode("utf-8")
        req = urllib.request.Request(
            f"{self.agent_api_url}/api/v1/eval",
            data=payload,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        with urllib.request.urlopen(req, timeout=30) as resp:
            result = json.loads(resp.read())
            if "error" in result:
                raise RuntimeError(result["error"])
            return result.get("result", result)


if __name__ == "__main__":
    from ipykernel.kernelapp import IPKernelApp
    IPKernelApp.launch_instance(kernel_class=AetherShellKernel)
