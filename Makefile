WASM_TARGET := wasm32v1-none
WASM_DIR    := target/$(WASM_TARGET)/release
CONTRACTS   := escrow stream vesting recurring batch_payout

.PHONY: all build test fmt fmt-check lint audit optimize specs clean ci coverage

all: build test

# The integration-test crate is host-only (it needs std), so it is excluded
# from the wasm build rather than failing it.
build:
	cargo build --workspace --exclude sororail-integration-tests \
		--target $(WASM_TARGET) --release

# Tests run on the host, not on wasm -- soroban_sdk::testutils needs std.
test:
	cargo nextest run --workspace

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

lint:
	cargo clippy --workspace --all-targets

audit:
	cargo audit

# Strips and shrinks each contract wasm. Requires the `stellar` CLI.
optimize: build
	@for c in $(CONTRACTS); do \
		echo "optimizing $$c"; \
		stellar contract optimize --wasm $(WASM_DIR)/sororail_$$c.wasm || exit 1; \
	done

# Contract specs as JSON, attached to the GitHub Release. Requires `stellar`
# and `jq`.
#
# Emitting a spec is not enough: a spec can reference a user-defined type it
# never defines (the `Payments` alias in DEPLOYMENTS.md), and `stellar
# contract info` prints it without complaint while every client that loads it
# fails with `Missing Entry`. So each spec must parse as JSON and every `udt`
# it references must be defined in it.
SPEC_CHECK := ([.[] | to_entries[] | select(.key | startswith("udt_")) | .value.name]) as $$defined \
	| ([.. | objects | select(has("udt")) | .udt.name] | unique) - $$defined \
	| if length > 0 then error("references undefined types: \(join(", "))") else true end

specs: build
	@mkdir -p dist/specs
	@for c in $(CONTRACTS); do \
		stellar contract info interface --output json \
			--wasm $(WASM_DIR)/sororail_$$c.wasm > dist/specs/$$c.json || exit 1; \
		jq -e '$(SPEC_CHECK)' dist/specs/$$c.json > /dev/null \
			|| { echo "invalid spec: $$c"; exit 1; }; \
	done
	@echo "specs written to dist/specs/"

ci: fmt-check lint test build

# Matches .github/workflows/ci.yml coverage job (lines / regions / functions).
coverage:
	cargo llvm-cov nextest --workspace \
		--fail-under-lines 90 \
		--fail-under-regions 75 \
		--fail-under-functions 80
	bash scripts/audit-error-variant-coverage.sh

clean:
	cargo clean
	rm -rf dist