.PHONY: build fix-wasm test lint fmt

build:
	cargo build --target wasm32-unknown-unknown --release
	$(MAKE) fix-wasm

fix-wasm:
	wasm-tools print target/wasm32-unknown-unknown/release/credit_factory.wasm \
		| wasm-tools parse -o target/wasm32-unknown-unknown/release/credit_factory.wasm -
	wasm-tools print target/wasm32-unknown-unknown/release/credit_token.wasm \
		| wasm-tools parse -o target/wasm32-unknown-unknown/release/credit_token.wasm -
	wasm-tools strip -d 'target_features' \
		-o target/wasm32-unknown-unknown/release/credit_factory.wasm \
		target/wasm32-unknown-unknown/release/credit_factory.wasm
	wasm-tools strip -d 'target_features' \
		-o target/wasm32-unknown-unknown/release/credit_token.wasm \
		target/wasm32-unknown-unknown/release/credit_token.wasm

test: build
	cargo test --workspace -- --nocapture

lint:
	cargo clippy --workspace -- -D warnings

fmt:
	cargo fmt --check
