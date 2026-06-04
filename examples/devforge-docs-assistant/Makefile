NAME := devforge-docs-assistant

.PHONY: dev build run clean

dev:
	@rm -f ui/.env.development
	@echo "Starting Arcanum backend on :8080 ..."
	ARCANUM_DEV=1 cargo run &
	@echo "Waiting for backend to write dev key..."
	@while [ ! -f ui/.env.development ]; do sleep 1; done
	@echo "Starting UI dev server on :5173 ..."
	cd ui && npm run dev

build:
	cd ui && npm run build
	cargo build --release

run:
	./target/release/$(NAME)

clean:
	rm -rf ui/dist ui/node_modules target data .arcanum-dev-key
