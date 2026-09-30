BIN := bin
AGENT_ENV := GOOS=linux GOARCH=amd64 CGO_ENABLED=0
PREFIX ?= $(HOME)/.local
VERSION := $(shell git describe --tags --always --dirty 2>/dev/null || echo dev)
COMMIT := $(shell git rev-parse --short HEAD 2>/dev/null || echo none)
DATE := $(shell date -u +%Y-%m-%dT%H:%M:%SZ)
LDFLAGS := -s -w -X main.version=$(VERSION) -X main.commit=$(COMMIT) -X main.date=$(DATE)
# Dev targets (release, e2e) use the repo .env; the installed `lobo` uses ~/.config/lobo/config.env.
DEV_CONFIG := --config $(CURDIR)/.env

.PHONY: build build-lobo build-agent install lint test release e2e mac install-mac dmg

build: build-lobo build-agent

build-lobo:
	go build -ldflags '$(LDFLAGS)' -o $(BIN)/lobo ./cmd/lobo

build-agent:
	$(AGENT_ENV) go build -trimpath -ldflags "-s -w" -o $(BIN)/lobo-agent ./cmd/lobo-agent

# Build and install `lobo` into $(PREFIX)/bin (default ~/.local/bin). Then: `lobo config`, `lobo up`.
install: build-lobo
	install -d $(PREFIX)/bin
	install -m 0755 $(BIN)/lobo $(PREFIX)/bin/lobo
	@echo "installed $(PREFIX)/bin/lobo ($(VERSION))"
	@case ":$$PATH:" in *":$(PREFIX)/bin:"*) ;; *) echo "⚠ $(PREFIX)/bin is not on PATH: add it to your shell profile";; esac
	@if [ -f .env ] && [ ! -f "$$($(BIN)/lobo config path)" ]; then \
		f="$$($(BIN)/lobo config path)"; install -d -m 0700 "$$(dirname "$$f")"; install -m 0600 .env "$$f"; \
		echo "copied repo .env to $$f"; fi

lint:
	golangci-lint run ./...

test:
	go test ./...

# Dev-only: publish the pod agent release to the bucket (needs R2 keys in .env).
release: build-lobo
	$(BIN)/lobo release $(DEV_CONFIG)

e2e: build-lobo
	go test -tags e2e -count=1 -v -timeout 30m ./e2e/

# macOS menu bar app: bundles the lobo CLI from this commit. Unsigned (ad-hoc), for this Mac.
MAC_APP := $(BIN)/lobocode.app
mac: build-lobo
	cd macos && swift build -c release
	if [ -d $(MAC_APP).tmp ]; then mkdir -p /tmp/trash && mv $(MAC_APP).tmp /tmp/trash/lobocode.app.tmp.$$(date +%s); fi
	mkdir -p $(MAC_APP).tmp/Contents/MacOS $(MAC_APP).tmp/Contents/Resources
	cp macos/.build/release/Lobocode $(MAC_APP).tmp/Contents/MacOS/Lobocode
	cp $(BIN)/lobo $(MAC_APP).tmp/Contents/Resources/lobo
	sed 's/__VERSION__/$(subst v,,$(VERSION))/' macos/Info.plist > $(MAC_APP).tmp/Contents/Info.plist
	macos/.build/release/Lobocode --render $(BIN)/lobo-renders >/dev/null
	python3 macos/make_icns.py $(BIN)/lobo-renders/icon_1024.png $(MAC_APP).tmp/Contents/Resources/AppIcon.icns
	codesign --force --deep -s - $(MAC_APP).tmp
	if [ -d $(MAC_APP) ]; then mkdir -p /tmp/trash && mv $(MAC_APP) /tmp/trash/lobocode.app.$$(date +%s); fi
	mv $(MAC_APP).tmp $(MAC_APP)
	@echo "built $(MAC_APP)"

# Drag-to-install disk image: lobocode.app next to an Applications shortcut. Unsigned (ad-hoc).
DMG := $(BIN)/lobocode.dmg
dmg: mac
	if [ -d $(BIN)/dmg ]; then mkdir -p /tmp/trash && mv $(BIN)/dmg /tmp/trash/lobocode-dmg.$$(date +%s); fi
	if [ -f $(DMG) ]; then mkdir -p /tmp/trash && mv $(DMG) /tmp/trash/lobocode.dmg.$$(date +%s); fi
	mkdir -p $(BIN)/dmg
	cp -R $(MAC_APP) $(BIN)/dmg/lobocode.app
	ln -s /Applications $(BIN)/dmg/Applications
	hdiutil create -volname lobocode -srcfolder $(BIN)/dmg -ov -format UDZO $(DMG) >/dev/null
	@echo "built $(DMG)"

# /Applications when writable (admin users), else ~/Applications.
APP_DIR := $(shell test -w /Applications && echo /Applications || echo $(HOME)/Applications)
install-mac: mac
	mkdir -p $(APP_DIR)
	if [ -d $(APP_DIR)/lobocode.app ]; then mkdir -p /tmp/trash && mv $(APP_DIR)/lobocode.app /tmp/trash/lobocode.app.installed.$$(date +%s); fi
	cp -R $(MAC_APP) $(APP_DIR)/lobocode.app
	@echo "installed $(APP_DIR)/lobocode.app: open it once, it lives in the menu bar"

.PHONY: rust-build rust-test rust-lint proto-fixtures proto-ts
rust-build:
	cargo build --workspace --locked

rust-test:
	cargo test --workspace --locked --features lobo-cli/test-fakes

rust-lint:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets --locked --features lobo-cli/test-fakes -- -D warnings

proto-fixtures:
	go run ./tools/protofixtures crates/lobo-proto/fixtures crates/lobo-proto/catalog.json

proto-ts:
	cargo test --locked -p lobo-proto export_bindings

.PHONY: rust-agent
rust-agent:
	cargo zigbuild --release --locked -p lobo-agent --target x86_64-unknown-linux-musl
	file target/x86_64-unknown-linux-musl/release/lobo-agent | grep -Eq 'statically linked|static-pie linked'
	ls -l target/x86_64-unknown-linux-musl/release/lobo-agent

.PHONY: core-fixtures
core-fixtures:
	go run ./tools/corefixtures dump crates/lobo-core/fixtures

.PHONY: cli-fixtures
cli-fixtures:
	python3 tools/clifixtures/capture.py all crates/lobo-cli/tests/fixtures/go
	TZ=UTC go test -tags capture -run TestCapture ./cmd/lobo/ -args -out $(CURDIR)/crates/lobo-cli/tests/fixtures/go/text

.PHONY: rust-build-lobo rust-install rust-release-snapshot
rust-build-lobo:
	LOBO_VERSION=$(VERSION) LOBO_COMMIT=$(COMMIT) LOBO_DATE=$(DATE) cargo build --release --locked -p lobo-cli
	install -d $(BIN)
	install -m 0755 target/release/lobo $(BIN)/lobo-rs

rust-install: rust-build-lobo
	install -d $(PREFIX)/bin
	install -m 0755 $(BIN)/lobo-rs $(PREFIX)/bin/lobo-rs
	@echo "installed $(PREFIX)/bin/lobo-rs ($(VERSION))"

rust-release-snapshot:
	HOMEBREW_TAP_KEY= CARGO_BUILD_JOBS=1 goreleaser release --snapshot --clean --skip=publish --parallelism=1
