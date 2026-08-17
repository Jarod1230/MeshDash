# Common workflows. Run `just` for the list.
# Install: cargo install just — or read the recipes and run the commands directly.

default:
    @just --list

# Everything CI runs, in the same order.
check: check-rust check-web check-docs

check-rust:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace

check-web:
    cd web && pnpm lint
    cd web && pnpm typecheck
    cd web && pnpm test
    cd web && pnpm build

check-docs:
    python3 scripts/check-links.py

# Apply formatting instead of just checking it.
fmt:
    cargo fmt --all

# Backend, listening on 127.0.0.1:8080 once step 5 of the roadmap lands.
dev-server:
    cargo run -p meshdash-server

# Frontend with hot reload, proxying /api to the backend.
dev-web:
    cd web && pnpm dev

# Install frontend dependencies.
setup:
    cd web && pnpm install

# Release build: one binary with the dashboard inside.
# The frontend must be built first — `embed-frontend` reads web/dist.
build:
    cd web && pnpm build
    cargo build --release --features meshdash-server/embed-frontend
