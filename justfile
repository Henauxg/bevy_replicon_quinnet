set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

tic_tac_toe_server_cmd := "cargo run --example tic_tac_toe -- server"
tic_tac_toe_client_cmd := "cargo run --example tic_tac_toe -- client"
boids_server_cmd := "cargo run --example deterministic_boids -- server"
boids_client_cmd := "cargo run --example deterministic_boids -- client"

default:
    @just --list

# Mini local CI
check:
    @echo "==> fmt"
    cargo fmt --all -- --check
    @echo "==> clippy"
    cargo clippy --all-features --locked -- -D warnings
    @echo "==> check"
    cargo check --all-features --locked
    @echo "==> test"
    cargo test --all-features --locked
    @echo "==> docs"
    cargo doc --no-deps --all-features --locked
    @echo "==> machete"
    cargo machete

# Format all Rust code
format:
    cargo fmt --all

# Open a command in a new terminal window (Linux/macOS)
[unix]
[private]
_unix-terminal command:
    #!/usr/bin/env bash
    set -euo pipefail
    root='{{justfile_directory()}}'
    command='{{command}}'
    if command -v gnome-terminal &>/dev/null; then
        gnome-terminal -- bash -c "cd '$root' && $command; exec bash"
    elif command -v open &>/dev/null && [[ "$(uname)" == "Darwin" ]]; then
        open -na Terminal --args bash -c "cd '$root' && $command; exec bash"
    else
        exit 1
    fi

# Launch tic-tac-toe server and client in separate windows
[windows]
tic-tac-toe:
    $root = '{{justfile_directory()}}'
    Start-Process powershell -ArgumentList '-NoExit', '-Command', "Set-Location '$root'; {{tic_tac_toe_server_cmd}}"
    Start-Sleep -Seconds 2
    Start-Process powershell -ArgumentList '-NoExit', '-Command', "Set-Location '$root'; {{tic_tac_toe_client_cmd}}"
    Write-Host 'Tic-tac-toe: server and client launched in separate windows.'

[unix]
tic-tac-toe:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! just _unix-terminal "{{tic_tac_toe_server_cmd}}"; then
        echo 'Run these in separate terminals:'
        echo '  {{tic_tac_toe_server_cmd}}'
        echo '  {{tic_tac_toe_client_cmd}}'
        exit 0
    fi
    sleep 2
    just _unix-terminal "{{tic_tac_toe_client_cmd}}"

# Launch deterministic boids server and client in separate windows
[windows]
boids:
    $root = '{{justfile_directory()}}'
    Start-Process powershell -ArgumentList '-NoExit', '-Command', "Set-Location '$root'; {{boids_server_cmd}}"
    Start-Sleep -Seconds 2
    Start-Process powershell -ArgumentList '-NoExit', '-Command', "Set-Location '$root'; {{boids_client_cmd}}"
    Write-Host 'Boids: server and client launched in separate windows.'

[unix]
boids:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! just _unix-terminal "{{boids_server_cmd}}"; then
        echo 'Run these in separate terminals:'
        echo '  {{boids_server_cmd}}'
        echo '  {{boids_client_cmd}}'
        exit 0
    fi
    sleep 2
    just _unix-terminal "{{boids_client_cmd}}"
