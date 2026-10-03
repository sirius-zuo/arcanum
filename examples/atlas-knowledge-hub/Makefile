NAME := atlas-knowledge-hub

.PHONY: dev build run test smoke clean

# Backend in the background, UI dev server in the foreground. The trap stops the backend
# when the UI server exits or on Ctrl-C.
dev:
	@echo "Starting backend on :8080 (UI dev server on :5173, Ctrl-C stops both) ..."
	@trap 'kill 0' INT TERM EXIT; \
	cargo run & \
	cd ui && npm run dev

build:
	npm ci --prefix ui && npm run build --prefix ui && cargo build --release

run:
	./target/release/$(NAME)

test:
	cargo test && npm test --prefix ui

smoke:
	node scripts/smoke.mjs

clean:
	rm -rf ui/dist ui/node_modules target data .arcanum-dev-key
