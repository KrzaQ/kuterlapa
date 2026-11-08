# Kuterlapa: control a Mac by voice and a game controller.
#
# Machine-specific values (a code signing identity, for example) go in
# Makefile.local, which git ignores. See Makefile.local.example.

.PHONY: help build test speech install run stop logs hooks check-private clean
.DEFAULT_GOAL := help

# Code signing identity. "-" signs ad hoc, which works on any Mac but
# makes macOS forget the app's permissions after every rebuild. A stable
# identity (a self-signed code signing certificate is enough) keeps them.
SIGN_IDENTITY ?= -
BUNDLE_ID ?= cc.krzaq.kuterlapa
VERSION ?= 0.1.0
APP_DIR ?= $(HOME)/Applications
BIN_DIR ?= $(HOME)/.local/bin
LOG_FILE ?= $(HOME)/Library/Logs/Kuterlapa.log
CONFIG_DIR ?= $(or $(XDG_CONFIG_HOME),$(HOME)/.config)/kuterlapa
# Speech recognition backend to install: mlx (Apple Silicon) or faster-whisper.
SPEECH_BACKEND ?= mlx
# Extra for the voice provider: empty (say, command) or mlx (local models).
VOICE_EXTRA ?=

APP := build/Kuterlapa.app

help: ## Show this help
	@grep -hE '^[a-zA-Z0-9_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort -t: -k1,1 | \
		awk 'BEGIN {FS = ":.*?## "; w = 12} { k[NR] = $$1; v[NR] = $$2; if (length($$1) > w) w = length($$1) } \
		     END { for (i = 1; i <= NR; i++) printf "  \033[36m%-*s\033[0m  %s\n", w, k[i], v[i] }'

build: build/AppIcon.icns ## Build the core and the macOS app, and sign build/Kuterlapa.app
	cd core && cargo build --release
	swift build -c release
	rm -rf $(APP)
	mkdir -p $(APP)/Contents/MacOS $(APP)/Contents/Helpers $(APP)/Contents/Resources
	cp build/AppIcon.icns $(APP)/Contents/Resources/AppIcon.icns
	cp .build/release/Kuterlapa $(APP)/Contents/MacOS/Kuterlapa
	# Helpers/, not MacOS/: the default macOS file system ignores case, so
	# "kuterlapa" next to "Kuterlapa" would overwrite the app.
	cp core/target/release/kuterlapa $(APP)/Contents/Helpers/kuterlapa
	sed -e 's/@BUNDLE_ID@/$(BUNDLE_ID)/' -e 's/@VERSION@/$(VERSION)/' \
		Resources/Info.plist > $(APP)/Contents/Info.plist
	codesign --force --sign '$(SIGN_IDENTITY)' --identifier '$(BUNDLE_ID).core' \
		$(APP)/Contents/Helpers/kuterlapa
	codesign --force --sign '$(SIGN_IDENTITY)' --identifier '$(BUNDLE_ID)' $(APP)

build/AppIcon.icns: scripts/make-icon.swift
	rm -rf build/AppIcon.iconset
	swift scripts/make-icon.swift build/AppIcon.iconset
	iconutil -c icns build/AppIcon.iconset -o $@

test: ## Run the core's tests
	cd core && cargo test

speech: ## Set up the speech provider's environment with SPEECH_BACKEND (default mlx)
	cd speech && uv sync --extra $(SPEECH_BACKEND)

install: build ## Install the app, the CLI, the speech and voice providers and example settings
	mkdir -p $(APP_DIR) $(BIN_DIR)
	rm -rf $(APP_DIR)/Kuterlapa.app
	cp -R $(APP) $(APP_DIR)/Kuterlapa.app
	ln -sf $(APP_DIR)/Kuterlapa.app/Contents/Helpers/kuterlapa $(BIN_DIR)/kuterlapa
	uv tool install --force --reinstall './speech[$(SPEECH_BACKEND)]'
	uv tool install --force --reinstall './voice$(if $(VOICE_EXTRA),[$(VOICE_EXTRA)])'
	mkdir -p $(CONFIG_DIR)
	for f in config.toml mapping.json profiles.json submit.json speech.toml voice.toml dictionary.txt replacements.txt; do \
		[ -e $(CONFIG_DIR)/$$f ] || cp config/$$f.example $(CONFIG_DIR)/$$f; \
	done

run: ## Start the installed app, logging to LOG_FILE (default ~/Library/Logs/Kuterlapa.log)
	open --stdout $(LOG_FILE) --stderr $(LOG_FILE) $(APP_DIR)/Kuterlapa.app

stop: ## Quit the app and wait until it is gone (the core and the providers stop with it)
	osascript -e 'quit app "Kuterlapa"'
	@for i in $$(seq 1 50); do pgrep -qf 'Kuterlapa.app/Contents/MacOS' || exit 0; sleep 0.2; done; \
		echo "Kuterlapa did not quit in 10 s" >&2; exit 1

logs: ## Follow the app log
	tail -f $(LOG_FILE)

hooks: ## Install the git pre-commit hook that blocks private text
	ln -sf ../../scripts/check-private.sh .git/hooks/pre-commit

check-private: ## Check the staged changes for private text
	scripts/check-private.sh

clean: ## Remove build output and the speech provider's environment
	rm -rf .build build core/target speech/.venv

-include Makefile.local
