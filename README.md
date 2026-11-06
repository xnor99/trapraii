# trapraii

Runs two commands sequentially, waiting for a termination signal (SIGINT, SIGHUP, SIGTERM) between them.

If the first command fails, trapraii exits immediately with its exit code. If it succeeds, trapraii waits until a signal arrives before running the second command.

## Usage

```bash
trapraii [--magic-error-code N] command1 command2
```

Arguments:
- `command1`, `command2` — space-separated shell command strings
- `--magic-error-code` — fallback exit code if parsing fails (default: 42)

## Example

```bash
trapraii "ufw allow 1234" "ufw delete allow 1234"
```

This opens a firewall port, waits for SIGINT (Ctrl+C), then removes the rule.
