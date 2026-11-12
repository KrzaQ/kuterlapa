# Kuterlapa: control a Mac by voice and a game controller.
#
# Machine-specific values (a code signing identity, for example) go in
# Makefile.local, which git ignores. See Makefile.local.example.

.PHONY: help build test speech install run stop logs push hooks check-private screenshots clean
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
SPEECH_SOCKET ?= $(or $(XDG_STATE_HOME),$(HOME)/.local/state)/kuterlapa/speech.sock
CONFIG_DIR ?= $(or $(XDG_CONFIG_HOME),$(HOME)/.config)/kuterlapa
DATA_DIR ?= $(or $(XDG_DATA_HOME),$(HOME)/.local/share)/kuterlapa
# Speech recognition backend to install: mlx (Apple Silicon) or faster-whisper.
SPEECH_BACKEND ?= mlx
# Extra for the voice provider: empty (say, command) or mlx (local models).
VOICE_EXTRA ?=
# Extra for the embedding provider: mlx (a local model) or empty (only an
# OpenAI-compatible server).
EMBED_EXTRA ?= mlx
# The git remotes that `make push` pushes main to.
REMOTES ?= origin

APP := build/Kuterlapa.app

help: ## Show this help
	@grep -hE '^[a-zA-Z0-9_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort -t: -k1,1 | \
		awk 'BEGIN {FS = ":.*?## "; w = 12} { k[NR] = $$1; v[NR] = $$2; if (length($$1) > w) w = length($$1) } \
		     END { for (i = 1; i <= NR; i++) printf "  \033[36m%-*s\033[0m  %s\n", w, k[i], v[i] }'

build: lejce/Cargo.toml build/AppIcon.icns ## Build the core and the macOS app, and sign build/Kuterlapa.app
	cd core && cargo build --release
	swift build -c release
	rm -rf $(APP)
	mkdir -p $(APP)/Contents/MacOS $(APP)/Contents/Helpers $(APP)/Contents/Resources
	cp build/AppIcon.icns $(APP)/Contents/Resources/AppIcon.icns
	cp -R view $(APP)/Contents/Resources/
	cp .build/release/Kuterlapa $(APP)/Contents/MacOS/Kuterlapa
	# Helpers/, not MacOS/: the default macOS file system ignores case, so
	# "kuterlapa" next to "Kuterlapa" would overwrite the app.
	cp core/target/release/kuterlapa $(APP)/Contents/Helpers/kuterlapa
	sed -e 's/@BUNDLE_ID@/$(BUNDLE_ID)/' -e 's/@VERSION@/$(VERSION)/' \
		Resources/Info.plist > $(APP)/Contents/Info.plist
	codesign --force --sign '$(SIGN_IDENTITY)' --identifier '$(BUNDLE_ID).core' \
		$(APP)/Contents/Helpers/kuterlapa
	codesign --force --sign '$(SIGN_IDENTITY)' --identifier '$(BUNDLE_ID)' $(APP)

# The core needs Lejce, a git submodule. A clone without
# --recurse-submodules has an empty folder; fetch it then.
lejce/Cargo.toml:
	git submodule update --init lejce

build/AppIcon.icns: scripts/make-icon.swift
	rm -rf build/AppIcon.iconset
	swift scripts/make-icon.swift build/AppIcon.iconset
	iconutil -c icns build/AppIcon.iconset -o $@

test: lejce/Cargo.toml ## Run the core's tests and check that the Python providers load
	cd core && cargo test
	cd speech && python3 -c "import ast, pathlib; [compile(p.read_text(), str(p), 'exec') for p in pathlib.Path('src').rglob('*.py')]"
	cd speech/src && python3 -c "from kuterlapa_speech.text import build_prompt, clean, context_names"
	cd voice && python3 -c "import ast, pathlib; [compile(p.read_text(), str(p), 'exec') for p in pathlib.Path('src').rglob('*.py')]"
	cd embed && python3 -c "import ast, pathlib; [compile(p.read_text(), str(p), 'exec') for p in pathlib.Path('src').rglob('*.py')]"

speech: ## Set up the speech provider's environment with SPEECH_BACKEND (default mlx)
	cd speech && uv sync --extra $(SPEECH_BACKEND)

install: build ## Install the app, the CLI, the providers (speech, voice, embed), the macOS tool pack and example settings
	mkdir -p $(APP_DIR) $(BIN_DIR)
	rm -rf $(APP_DIR)/Kuterlapa.app
	cp -R $(APP) $(APP_DIR)/Kuterlapa.app
	ln -sf $(APP_DIR)/Kuterlapa.app/Contents/Helpers/kuterlapa $(BIN_DIR)/kuterlapa
	uv tool install --force --reinstall './speech[$(SPEECH_BACKEND)]'
	uv tool install --force --reinstall './voice$(if $(VOICE_EXTRA),[$(VOICE_EXTRA)])'
	uv tool install --force --reinstall './embed$(if $(EMBED_EXTRA),[$(EMBED_EXTRA)])'
	mkdir -p $(DATA_DIR)/tools
	cp tools/macos/* $(DATA_DIR)/tools/
	mkdir -p $(CONFIG_DIR)
	for f in config.toml mapping.json profiles.json submit.json speech.toml voice.toml embed.toml dictionary.txt replacements.txt; do \
		[ -e $(CONFIG_DIR)/$$f ] || cp config/$$f.example $(CONFIG_DIR)/$$f; \
	done

run: ## Start the installed app, logging to LOG_FILE (default ~/Library/Logs/Kuterlapa.log)
	open --stdout $(LOG_FILE) --stderr $(LOG_FILE) $(APP_DIR)/Kuterlapa.app

stop: ## Quit the app and wait until it is gone (the core and the providers stop with it)
	@# A dictation that runs now would be lost: wait for it to end (at most
	@# two minutes), and a moment more for its text to be typed.
	@for i in $$(seq 1 240); do \
		state=$$(echo status | nc -U $(SPEECH_SOCKET) 2>/dev/null); \
		if [ -z "$$state" ] || [ "$$state" = idle ]; then \
			if [ $$i -gt 1 ]; then sleep 2; fi; break; \
		fi; \
		if [ $$i -eq 1 ]; then echo "waiting for the dictation to end ($$state)"; fi; \
		sleep 0.5; \
	done
	osascript -e 'quit app "Kuterlapa"'
	@for i in $$(seq 1 50); do pgrep -qf 'Kuterlapa.app/Contents/MacOS' || exit 0; sleep 0.2; done; \
		echo "Kuterlapa did not quit in 10 s" >&2; exit 1

screenshots: ## Retake docs/screenshots/ with example content (Kuterlapa must run)
	python3 scripts/screenshots.py

logs: ## Follow the app log
	tail -f $(LOG_FILE)

push: ## Push main to every remote in REMOTES (default origin)
	for r in $(REMOTES); do git push $$r main || exit 1; done

hooks: ## Install the git pre-commit hook that blocks private text
	ln -sf ../../scripts/check-private.sh .git/hooks/pre-commit

check-private: ## Check the staged changes for private text
	scripts/check-private.sh

clean: ## Remove build output and the speech provider's environment
	rm -rf .build build core/target speech/.venv

-include Makefile.local
