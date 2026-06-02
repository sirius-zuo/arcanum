NAME := folio-library-search

.PHONY: dev build run clean

dev:
	@echo "Starting Arcanum backend on :8080 ..."
	cargo run &
	@echo "Starting UI dev server on :5173 ..."
	cd ui && npm run dev

build:
	cd ui && npm run build
	cargo build --release

run:
	./target/release/$(NAME)

clean:
	rm -rf ui/dist ui/node_modules target data .arcanum-dev-key
