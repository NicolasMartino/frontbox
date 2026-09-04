# frontbox tasks.
#
# Requires: Docker for the example stack; a browser and a version-matched
# chromedriver for the IndexedDB suite (`just chromedriver` fetches one); Xcode
# for iOS and an NDK for Android.
#
# # Why the ports are variables and the URLs are derived
#
# `platform::BASE_URL` and `USER_BASE_URL` read their values through
# `option_env!`, so the service addresses are compiled into the binary — a wasm
# bundle or a phone build has no environment to consult at run time. Writing them
# as literals anywhere means a stack where every service is healthy and the
# application silently talks to the wrong port, and there is no error for that: a
# fetch to a dead port fails the way being offline does, which is the one
# condition this application is designed to treat as normal.
#
# So every recipe below derives the URL from the port. Override the port, not the
# URL.

todo_port := env("TODO_SERVER_PORT", "3000")
user_port := env("USER_SERVER_PORT", "3001")
app_port := env("TODO_APP_PORT", "8081")

# The devices D4c created. Named rather than "first available" so a run does not
# quietly land on whatever simulator happened to be booted.
ios_device := env("IOS_DEVICE", "frontbox-iphone")
android_avd := env("ANDROID_AVD", "frontbox")

# List recipes.
default:
    @just --list

# The whole gate. This is `scripts/verify.sh` and nothing else, because the
# script is the definition of "verified" for this project and a second list of
# checks here would be a second thing to keep in step.
[doc("Every gate: fmt, clippy, tests, coverage, the wasm builds, the wiki checks.")]
check:
    bash scripts/verify.sh

# Run tests. arg: all (default) | core | backends | trial | doc
test arg="all":
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{ arg }}" in
      all)      cargo test --workspace --all-features ;;
      core)     cargo test -p frontbox --all-features ;;
      # IndexedDB is absent on purpose: it needs a browser, which is `just browser`.
      backends) cargo test -p frontbox-sqlite --features testing ;;
      trial)    cargo test -p todo-core -p todo-server -p user-server ;;
      doc)      cargo test --workspace --all-features --doc ;;
      *) echo "usage: just test [all|core|backends|trial|doc]" >&2; exit 1 ;;
    esac

# Fast gate: everything that needs no browser, no Docker and no phone.
lint:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --quiet

# Open an HTML coverage report for the core crate.
#
# `-p frontbox` rather than the workspace: the floor is a claim about the
# library, and folding in the examples would let a well-tested trial hide an
# untested core.
[doc("Open an HTML coverage report for the core crate.")]
cov-html:
    cargo llvm-cov -p frontbox --all-features --html --open

# Fetch a chromedriver matching the installed Chrome, into `target/`.
#
# The IndexedDB suite needs one and the version has to match Chrome's build, which
# is the friction that kept that suite unrun for two deliverables. Prints the
# export line rather than writing to a shell profile.
[doc("Download a chromedriver matching the installed Chrome.")]
chromedriver:
    #!/usr/bin/env bash
    set -euo pipefail
    chrome="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
    [ -x "$chrome" ] || { echo "no Google Chrome at $chrome" >&2; exit 1; }
    build=$("$chrome" --version | sed -E 's/[^0-9]*([0-9]+\.[0-9]+\.[0-9]+)\.[0-9]+.*/\1/')
    case "$(uname -m)" in arm64) plat=mac-arm64 ;; *) plat=mac-x64 ;; esac
    url=$(curl -sS https://googlechromelabs.github.io/chrome-for-testing/latest-patch-versions-per-build-with-downloads.json \
      | python3 -c "import json,sys;d=json.load(sys.stdin)['builds']['$build']['downloads']['chromedriver'];print(next(x['url'] for x in d if x['platform']=='$plat'))")
    mkdir -p target/chromedriver
    curl -sSL "$url" -o target/chromedriver/driver.zip
    unzip -oq target/chromedriver/driver.zip -d target/chromedriver
    driver=$(find target/chromedriver -name chromedriver -type f | head -1)
    chmod +x "$driver"
    xattr -d com.apple.quarantine "$driver" 2>/dev/null || true
    echo ""
    echo "export CHROMEDRIVER=$(pwd)/$driver"

# The IndexedDB conformance suite and the cross-realm fixture, in headless Chrome.
#
# Three environment variables, all load-bearing:
#   - CHROMEDRIVER            — `just chromedriver` prints the export line.
#   - the cargo runner        — without it cargo hands the `.wasm` to the shell,
#                               which answers `cannot execute binary file`, exit
#                               126. There is no `.cargo/config.toml` here.
#   - WASM_BINDGEN_TEST_ONLY_WEB — Node has no IndexedDB.
#
# `--release` is not optional: a debug build exceeds chromedriver's 300-second
# renderer timeout before a single test runs.
[doc("The IndexedDB suite in a real browser. Needs CHROMEDRIVER.")]
browser:
    #!/usr/bin/env bash
    set -euo pipefail
    [ -n "${CHROMEDRIVER:-}" ] || { echo "set CHROMEDRIVER — run: just chromedriver" >&2; exit 1; }
    CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
    WASM_BINDGEN_TEST_ONLY_WEB=1 \
      cargo test -p frontbox-indexeddb --release --target wasm32-unknown-unknown --features testing

# Drive the trial application.
#
# `all` is the Docker stack — two servers and the web UI, which is the only arm
# that needs no toolchain beyond Docker. The four platform arms run the *same*
# UI crate against those same servers, which is D4c's whole claim: nothing below
# `main.rs` is conditionally compiled.
#
# `servers` starts the two services alone, for when you want `dx serve` to own
# the UI. Every platform arm calls it first, so they are safe to run cold.
#
# `down` is the destructive arm: it removes the compose volumes. Both servers
# store in memory anyway, so what actually goes is the containers.
[doc("Run the trial. arg: all (default) | web | native | ios | android | e2e | servers | logs | down")]
examples arg="all":
    #!/usr/bin/env bash
    set -euo pipefail
    compose=(docker compose)
    # Exported for compose, which interpolates the published ports into the build
    # args the wasm bundle bakes. See the header.
    export TODO_SERVER_PORT="{{ todo_port }}" USER_SERVER_PORT="{{ user_port }}" TODO_APP_PORT="{{ app_port }}"

    servers() {
      "${compose[@]}" up -d --wait --wait-timeout 300 user-server todo-server
      echo "  todo-server  http://127.0.0.1:{{ todo_port }}/swagger-ui/"
      echo "  user-server  http://127.0.0.1:{{ user_port }}/swagger-ui/"
    }

    # The URLs a *host-side* build needs. Android overrides the host below: the
    # emulator guest has its own loopback, so 127.0.0.1 reaches the phone.
    host_urls() {
      export TODO_SERVER_URL="http://${1}:{{ todo_port }}"
      export USER_SERVER_URL="http://${1}:{{ user_port }}"
    }

    case "{{ arg }}" in
      all)
        "${compose[@]}" up -d --build --wait --wait-timeout 900
        echo ""
        echo "  UI           http://127.0.0.1:{{ app_port }}"
        echo "  todo-server  http://127.0.0.1:{{ todo_port }}/swagger-ui/"
        echo "  user-server  http://127.0.0.1:{{ user_port }}/swagger-ui/"
        echo ""
        echo "Sign up first — nothing todo-shaped is on screen until you do."
        echo "Tear it down with: just examples down"
        ;;
      servers) servers ;;
      web)
        servers
        host_urls 127.0.0.1
        cd examples/todo-app && dx serve --web --port "{{ app_port }}"
        ;;
      native|desktop)
        servers
        host_urls 127.0.0.1
        # Known defect: the drain loop stalls on Dioxus desktop after a few
        # seconds — the VirtualDom's executor stops being polled, measured at 3
        # ticks against a bare tokio task's 18 over the same 95 seconds. Web, iOS
        # and Android are unaffected. See open-decisions entry 20.
        echo "note: the drain loop stalls on desktop — register entry 20, not a stack problem"
        # **`--always-on-top false`, or the focus arm cannot be tested at all.**
        # `dioxus-desktop` floats a served window over everything by default
        # (`config.rs`: `always_on_top().unwrap_or(true)`), which is convenient
        # while editing and fatal here: the one thing this arm exists to exercise
        # is the window *losing* focus, and a window that cannot be covered
        # cannot be blurred by clicking past it.
        cd examples/todo-app && dx serve --platform desktop --always-on-top false
        ;;
      ios)
        servers
        # An iOS simulator shares the host's network stack, so the loopback
        # address is right there — unlike Android below.
        host_urls 127.0.0.1
        if ! xcrun simctl list devices booted | grep -q "{{ ios_device }}"; then
          echo "booting {{ ios_device }}"
          xcrun simctl boot "{{ ios_device }}"
        fi
        open -a Simulator
        cd examples/todo-app && dx serve --platform ios
        ;;
      android)
        servers
        # **`10.0.2.2`, not `127.0.0.1`.** The emulator guest has its own
        # loopback; the host is reachable at 10.0.2.2 and nowhere else. Getting
        # this wrong leaves the app permanently and silently "offline", which is
        # the one state it is designed to treat as normal.
        host_urls 10.0.2.2
        sdk="${ANDROID_HOME:-$HOME/Library/Android/sdk}"
        adb="$sdk/platform-tools/adb"
        if ! "$adb" devices | grep -q "emulator.*device$"; then
          echo "starting the {{ android_avd }} emulator"
          "$sdk/emulator/emulator" -avd "{{ android_avd }}" -no-snapshot-save > /dev/null 2>&1 &
          "$adb" wait-for-device
          # `wait-for-device` returns when adb can talk to it, which is well
          # before the launcher exists. Waiting for boot completion instead.
          until [ "$("$adb" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; do sleep 2; done
        fi
        cd examples/todo-app && dx serve --platform android
        ;;
      e2e)
        # The browser end-to-end run: the signup gate, user CRUD, nested todo
        # CRUD, the delete cascade asserted against the *todo service*, and a
        # reload. It asserts on console errors as loudly as on behaviour, which
        # is what caught the IndexedDB transaction leak nothing else could see.
        node -e "require.resolve('playwright')" 2>/dev/null || {
          echo "playwright is not installed — run: npm i -D playwright && npx playwright install chromium" >&2
          exit 1
        }
        # Restarted rather than assumed clean: both servers hold state in memory,
        # so the signup gate is unreachable while the user service still knows
        # this client's user id.
        "${compose[@]}" restart todo-server user-server
        sleep 5
        APP_URL="http://127.0.0.1:{{ app_port }}" \
        TODO_URL="http://127.0.0.1:{{ todo_port }}" \
        USER_URL="http://127.0.0.1:{{ user_port }}" \
          node examples/todo-app/e2e/ui.mjs
        ;;
      logs) "${compose[@]}" logs -f todo-server user-server ;;
      down) "${compose[@]}" down -v --remove-orphans ;;
      *) echo "usage: just examples [all|web|native|ios|android|e2e|servers|logs|down]" >&2; exit 1 ;;
    esac
