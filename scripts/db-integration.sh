#!/usr/bin/env bash
# Disposable Docker live tests on Linux or macOS, amd64 or arm64.
# Both compiler/test runner and database are capped to one CPU, sequential DBs.
# No host compiler/native client installation or published host port is needed.
set -euo pipefail
# Callers may enable bash tracing; never emit temporary database credentials.
set +x
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
command -v docker >/dev/null || { echo 'Docker is required' >&2; exit 2; }
mysql_image='mysql:9.7.4@sha256:df90258da46f6e7ff8574ff84f17618aba0765cac749928299c69fd4e7cb8ad8'
rust_image='rust:1.98.0-bookworm@sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922'
postgres_image='postgres:18.6@sha256:3a4264520e121176619f54ff867d12774bc81e95e7b84db58d63bccdb6144b81'
mysql_features=${BIDECIMAL_MYSQL_TEST_FEATURES:-mysql-all}
postgres_features=${BIDECIMAL_POSTGRES_TEST_FEATURES:-postgres-all}
stages=${1:-all}
case "$stages" in all|mysql|postgres) ;; *) echo "usage: $0 [all|mysql|postgres]" >&2; exit 2;; esac
case "$mysql_features" in mysql-all|mysql-sqlx|mysql-diesel|mysql-sync|mysql-async) ;; *) echo 'Unsupported MySQL feature selection' >&2; exit 2;; esac
case "$postgres_features" in postgres-all|postgres-sqlx|postgres-diesel|postgres-sync|postgres-async) ;; *) echo 'Unsupported PostgreSQL feature selection' >&2; exit 2;; esac
printf "HOST nofile soft=%s hard=%s\n" "$(ulimit -Sn)" "$(ulimit -Hn)"
# Bound the disposable compiler container independently of Docker daemon defaults.
runner_nofile=65536
run_root=$(mktemp -d "${TMPDIR:-/tmp}/bidecimal-db.XXXXXXXX")
# macOS /var is a symlink; Docker bind source uses its canonical absolute path.
run_root=$(cd "$run_root" && pwd -P)
run_id=$(basename "$run_root")
label_key='org.bidecimal.integration-run'
server="${run_id}-mysql"
runner="${run_id}-runner"
network="${run_id}-network"
rust_verified=0
server_attempted=0
runner_attempted=0
cleanup() {
    code=$?
    trap - EXIT INT TERM
    safe=1
    for container in "$runner" "$server"; do
        attempted=$server_attempted
        [[ $container == "$runner" ]] && attempted=$runner_attempted
        [[ $attempted == 1 ]] || continue
        actual=$(docker inspect -f "{{index .Config.Labels \"$label_key\"}}" "$container" 2>/dev/null || true)
        if [[ $actual == "$run_id" ]]; then
            if (( code != 0 )); then docker logs "$container" >&2 || true; fi
            docker rm -f "$container" >/dev/null || { echo "Could not remove $container" >&2; code=1; safe=0; }
        else
            echo "Could not verify ownership/state of $container; retaining its mounts" >&2
            code=1; safe=0
        fi
    done
    if [[ $safe != 1 ]]; then
        echo "Container cleanup incomplete. Retaining network $network and directory $run_root" >&2
        exit "$code"
    fi
    actual=$(docker network inspect -f "{{index .Labels \"$label_key\"}}" "$network" 2>/dev/null || true)
    if [[ $actual == "$run_id" ]]; then
        docker network rm "$network" >/dev/null || { echo "Could not remove $network" >&2; code=1; }
    fi
    # This helper can run only after this immutable Rust image is verified to
    # declare NO VOLUMEs. Otherwise rejected images could create anonymous ones.
    if [[ $rust_verified == 1 ]]; then
        docker run --rm --network none --label "$label_key=$run_id" --cpus=1 \
            --user 0 --entrypoint /bin/sh --mount "type=bind,src=$run_root,dst=/cleanup" \
            "$rust_image" -c 'rm -rf /cleanup/data /cleanup/tmp' \
            || { echo "Could not clean $run_root" >&2; code=1; }
    fi
    # Only this invocation's unique temporary directory; never global prune,
    # existing image deletion, or deletion of another invocation's resources.
    rm -rf -- "$run_root" 2>/dev/null || { echo "Could not remove $run_root" >&2; code=1; }
    exit "$code"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir "$run_root/data" "$run_root/tmp"
chmod 1777 "$run_root/tmp"
# Check ALL declared VOLUMEs so an anonymous volume cannot be created silently.
docker pull "$rust_image"
volumes=$(docker image inspect "$rust_image" --format '{{range $k, $v := .Config.Volumes}}{{$k}}{{println}}{{end}}')
[[ -z $volumes ]] || { echo "Unexpected Rust VOLUME declarations: $volumes" >&2; exit 1; }
rust_verified=1
printf 'MySQL image: %s\nPostgreSQL image: %s\nRust image: %s\nrun=%s\n' "$mysql_image" "$postgres_image" "$rust_image" "$run_id"
docker network create --label "$label_key=$run_id" "$network" >/dev/null
random_test_password() {
    local value
    value=$(od -An -N16 -tx1 /dev/urandom | tr -d '[:space:]') || return $?
    [[ ${#value} == 32 && $value != *[![:xdigit:]]* ]] || {
        echo "Could not generate a temporary database password" >&2
        return 1
    }
    printf '%s' "$value"
}
run_stage() {
    local test_password url_name db_password_env client_password_env
    local MYSQL_ROOT_PASSWORD POSTGRES_PASSWORD MYSQL_PWD
    local BIDECIMAL_TEST_MYSQL_URL BIDECIMAL_TEST_POSTGRES_URL
    server="${run_id}-${stage}"
    runner="${run_id}-${stage}-runner"
    if [[ $stage == mysql ]]; then
        image=$mysql_image; data_path=/var/lib/mysql; features=$mysql_features
    else
        image=$postgres_image; data_path=/var/lib/postgresql; features=$postgres_features
    fi
    mkdir "$run_root/data/$stage" || return $?
    docker pull "$image" || return $?
    volumes=$(docker image inspect "$image" --format '{{range $k, $v := .Config.Volumes}}{{$k}}{{println}}{{end}}') || return $?
    [[ $volumes == "$data_path" ]] || { echo "Unexpected $stage VOLUME declarations: $volumes" >&2; return 1; }
    # Each disposable database gets a fresh 128-bit password. Keep values in
    # local exported variables and pass only their names to Docker, never their
    # contents as command-line arguments or printed output.
    test_password=$(random_test_password) || return $?
    client_password_env=
    if [[ $stage == mysql ]]; then
        url_name=BIDECIMAL_TEST_MYSQL_URL
        db_password_env=MYSQL_ROOT_PASSWORD
        client_password_env=MYSQL_PWD
        MYSQL_ROOT_PASSWORD=$test_password
        MYSQL_PWD=$test_password
        BIDECIMAL_TEST_MYSQL_URL="mysql://root:${test_password}@${server}:3306/bidecimal_test"
    else
        url_name=BIDECIMAL_TEST_POSTGRES_URL
        db_password_env=POSTGRES_PASSWORD
        POSTGRES_PASSWORD=$test_password
        BIDECIMAL_TEST_POSTGRES_URL="postgres://bidecimal:${test_password}@${server}:5432/bidecimal_test"
    fi
    # Localize exported secrets so they disappear when this stage returns,
    # including on an early failure.
    export "$db_password_env" "$url_name"
    if [[ -n $client_password_env ]]; then export "$client_password_env"; fi
    # No host ports. Only this run's runner reaches the disposable DB.
    server_attempted=1
    if [[ $stage == mysql ]]; then
        docker create --name "$server" --label "$label_key=$run_id" --network "$network" \
            --cpus=1 --mount "type=bind,src=$run_root/data/$stage,dst=$data_path" \
            --mount "type=bind,src=$run_root/tmp,dst=/tmp" \
            -e MYSQL_ROOT_PASSWORD -e MYSQL_ROOT_HOST=% \
            -e MYSQL_DATABASE=bidecimal_test "$image" --mysqlx=OFF >/dev/null || return $?
    else
        docker create --name "$server" --label "$label_key=$run_id" --network "$network" \
            --cpus=1 --mount "type=bind,src=$run_root/data/$stage,dst=$data_path" \
            --mount "type=bind,src=$run_root/tmp,dst=/tmp" \
            -e POSTGRES_PASSWORD -e POSTGRES_USER=bidecimal \
            -e POSTGRES_DB=bidecimal_test "$image" >/dev/null || return $?
    fi
    docker start "$server" >/dev/null || return $?
    ready=0
    for _ in $(seq 1 180); do
        # TCP readiness excludes both images' temporary bootstrap socket server.
        if [[ $stage == mysql ]]; then
            if docker exec -e MYSQL_PWD "$server" \
                mysqladmin ping --host=127.0.0.1 -uroot --silent >/dev/null 2>&1; then ready=1; break; fi
        else
            if docker exec "$server" pg_isready --host=127.0.0.1 -U bidecimal -d bidecimal_test >/dev/null 2>&1; then ready=1; break; fi
        fi
        [[ $(docker inspect -f '{{.State.Running}}' "$server") == true ]] || break
        sleep 1
    done
    [[ $ready == 1 ]] || { echo "$stage failed to become ready" >&2; return 1; }
    unset test_password "$db_password_env"
    if [[ -n $client_password_env ]]; then unset "$client_password_env"; fi
    runner_attempted=1
    docker create --name "$runner" --label "$label_key=$run_id" --network "$network" \
        --cpus=1 --ulimit "nofile=$runner_nofile:$runner_nofile" \
        --mount "type=bind,src=$root,dst=/workspace,readonly" \
        -w /workspace -e RUSTUP_TOOLCHAIN=1.98.0 -e CARGO_HOME=/cargo -e CARGO_TARGET_DIR=/target -e CARGO_BUILD_JOBS=1 \
        -e "$url_name" \
        -e "BIDECIMAL_DB_TEST_FEATURES=$features" -e "BIDECIMAL_DB_STAGE=$stage" \
        "$rust_image" /bin/bash -euc '
            if [[ "$BIDECIMAL_DB_STAGE" == mysql ]]; then
                tests=(sqlx_mysql diesel_mysql mysql_sync mysql_async)
                selections=(mysql-sqlx mysql-diesel mysql-sync mysql-async)
            else
                tests=(sqlx_postgres diesel_postgres postgres_sync tokio_postgres)
                selections=(postgres-sqlx postgres-diesel postgres-sync postgres-async)
            fi
            completed=()
            report_unrun() {
                code=$?
                trap - EXIT
                for i in "${!tests[@]}"; do
                    [[ "$BIDECIMAL_DB_TEST_FEATURES" == "$BIDECIMAL_DB_STAGE-all" || "$BIDECIMAL_DB_TEST_FEATURES" == "${selections[$i]}" ]] || continue
                    if [[ ${completed[$i]:-0} != 1 ]]; then
                        echo "RESULT $BIDECIMAL_DB_STAGE/${tests[$i]} BLOCKED (failure): runner setup/build interrupted"
                        code=1
                    fi
                done
                exit "$code"
            }
            trap report_unrun EXIT
            # Best-effort current /proc snapshots, not a compiler peak measurement.
            # Only still-alive named processes can be observed; a failed rustc or
            # linker may already have exited. Counts can race process exit.
            # Shell builtins still provide limits if spawning a tool fails.
            fd_snapshot() (
                set +e
                shopt -s nullglob
                printf "FD_SNAPSHOT stage=%s point=%s shell_pid=%s soft=%s hard=%s\n" \
                    "$BIDECIMAL_DB_STAGE" "$1" "$$" "$(ulimit -Sn)" "$(ulimit -Hn)"
                for metric in /proc/sys/fs/file-nr /proc/sys/fs/file-max /proc/sys/fs/nr_open; do
                    if [[ -r $metric ]]; then
                        value=unavailable
                        IFS= read -r value < "$metric"
                        printf "FD_KERNEL %s=%s\n" "$metric" "$value"
                    fi
                done
                echo "FD_NOTE current live process snapshot only; exited compiler/linker peaks are unavailable"
                count=0
                for process in /proc/[0-9]*; do
                    [[ -r $process/comm ]] || continue
                    IFS= read -r comm < "$process/comm" || continue
                    case "$comm" in bash|cargo|rustc|rust-lld|ld|ld.lld|cc|cc1|cc1plus) ;; *) continue;; esac
                    fds=("$process"/fd/*)
                    printf "FD_PROCESS pid=%s comm=%s observed_fds=%s\n" "${process##*/}" "$comm" "${#fds[@]}"
                    if [[ -r $process/limits ]]; then
                        while IFS= read -r line; do
                            case "$line" in "Max open files"*) printf "FD_LIMIT %s %s\n" "${process##*/}" "$line";; esac
                        done < "$process/limits"
                    fi
                    count=$((count + 1))
                    [[ $count -lt 32 ]] || break
                done
                return 0
            )
            fd_snapshot startup || true
            # Official Debian packages, in this disposable build container only.
            # Production adapters use backend-only features without native links.
            apt-get update
            apt-get install -y --no-install-recommends default-libmysqlclient-dev libpq-dev pkg-config cmake build-essential libssl-dev curl ca-certificates
            native_status=0
            if [[ "$BIDECIMAL_DB_STAGE" == mysql && ( "$BIDECIMAL_DB_TEST_FEATURES" == mysql-all || "$BIDECIMAL_DB_TEST_FEATURES" == mysql-diesel ) ]]; then
                if bash db-integration/install-native-client.sh /tmp/bidecimal-native-client; then
                    export MYSQLCLIENT_VERSION=3.3.21
                    export PATH=/tmp/bidecimal-native-client/bin:$PATH
                    export MYSQLCLIENT_LIB_DIR=/tmp/bidecimal-native-client/lib
                    export PKG_CONFIG_PATH=/tmp/bidecimal-native-client/lib/pkgconfig
                    export LD_LIBRARY_PATH=/tmp/bidecimal-native-client/lib
                    for pc in mysqlclient libmariadb; do
                        pc_lib=$(pkg-config --variable=libdir "$pc") || pc_lib=
                        if [[ $(pkg-config --modversion "$pc") != 3.3.21 || ${pc_lib%/} != /tmp/bidecimal-native-client/lib ]]; then
                            native_status=1
                            echo "RESULT mysql/diesel BLOCKED: wrong pkg-config resolution for $pc" >&2
                        fi
                    done
                else
                    native_status=1
                    echo "RESULT mysql/diesel BLOCKED: pinned native-client installation failed" >&2
                fi
            fi
            rustc --version; cargo --version; uname -a; sha256sum Cargo.lock
            dpkg --print-architecture
            dpkg-query -W libmariadb3 libmariadb-dev libmariadb-dev-compat default-libmysqlclient-dev libpq5 libpq-dev
            mysql_config --version || true
            mariadb_config --version || true
            pkg-config --modversion mysqlclient libmariadb libpq || true
            printf "MYSQLCLIENT_VERSION=%s MYSQLCLIENT_LIB_DIR=%s MYSQLCLIENT_LIBNAME=%s PKG_CONFIG_PATH=%s\n" \
                "${MYSQLCLIENT_VERSION:-unset-auto-detect}" "${MYSQLCLIENT_LIB_DIR:-unset}" \
                "${MYSQLCLIENT_LIBNAME:-unset}" "${PKG_CONFIG_PATH:-unset}"
            if [[ "$BIDECIMAL_DB_STAGE" == mysql && ( "$BIDECIMAL_DB_TEST_FEATURES" == mysql-all || "$BIDECIMAL_DB_TEST_FEATURES" == mysql-diesel ) ]]; then
                if cc -Wall -Wextra $(pkg-config --cflags libmariadb) \
                    db-integration/native-client-rebind.c $(pkg-config --libs libmariadb) \
                    -o /tmp/native-client-rebind && ldd /tmp/native-client-rebind && \
                    ldd /tmp/native-client-rebind | grep -F "/tmp/bidecimal-native-client/lib/libmariadb.so" && \
                    /tmp/native-client-rebind; then
                    :
                else
                    native_status=1
                    echo "RESULT mysql/diesel BLOCKED: native-client preflight failed" >&2
                fi
            fi
            failed=0
            for i in "${!tests[@]}"; do
                [[ "$BIDECIMAL_DB_TEST_FEATURES" == "$BIDECIMAL_DB_STAGE-all" || "$BIDECIMAL_DB_TEST_FEATURES" == "${selections[$i]}" ]] || continue
                test_name=${tests[$i]}
                completed[$i]=1
                if [[ "$test_name" == diesel_mysql && $native_status != 0 ]]; then
                    echo "RESULT $BIDECIMAL_DB_STAGE/$test_name BLOCKED (failure): native-client setup/preflight"
                    failed=1
                    continue
                fi
                fd_snapshot "before-build/$test_name" || true
                if cargo test --locked -p bidecimal-db-integration --no-default-features \
                    --features "${selections[$i]}" --test "$BIDECIMAL_DB_STAGE" --no-run \
                    --message-format=json > /tmp/driver-build.json; then
                    :
                else
                    compile_status=$?
                    fd_snapshot "compile-failed/$test_name/exit=$compile_status" || true
                    cat /tmp/driver-build.json || true
                    echo "RESULT $BIDECIMAL_DB_STAGE/$test_name FAIL: compile exit=$compile_status"
                    failed=1
                    continue
                fi
                executable=$(sed -n "s/.*\"executable\":\"\([^\"]*\)\".*/\1/p" /tmp/driver-build.json | tail -n 1)
                if [[ -z $executable || ! -x $executable ]]; then
                    echo "RESULT $BIDECIMAL_DB_STAGE/$test_name FAIL: missing compiled test executable"
                    failed=1
                    continue
                fi
                echo "Test executable: $executable"
                ldd "$executable" || true
                if [[ $test_name == diesel_mysql ]]; then
                    find /target/debug/build -path "*/mysqlclient-sys-*/output" -type f -print -exec cat {} \;
                    if ! ldd "$executable" | grep -F "/tmp/bidecimal-native-client/lib/libmariadb.so"; then
                        echo "RESULT mysql/$test_name BLOCKED (failure): wrong native library resolution"
                        failed=1
                        continue
                    fi
                fi
                if ! "$executable" --list | grep -Fx "$test_name: test"; then
                    echo "RESULT $BIDECIMAL_DB_STAGE/$test_name FAIL: expected test absent from executable"
                    failed=1
                    continue
                fi
                if "$executable" "$test_name" --exact --test-threads=1 --nocapture; then
                    echo "RESULT $BIDECIMAL_DB_STAGE/$test_name PASS"
                else
                    code=$?
                    fd_snapshot "test-failed/$test_name/exit=$code" || true
                    echo "RESULT $BIDECIMAL_DB_STAGE/$test_name FAIL exit=$code"
                    failed=1
                fi
                if [[ "$BIDECIMAL_DB_STAGE" == mysql ]]; then
                    if ! "$executable" observation_decimal_scientific_normalization --exact --nocapture; then
                        failed=1
                        echo "RESULT mysql/$test_name/observation_decimal_scientific_normalization FAIL"
                    fi
                fi
            done
            exit "$failed"
        ' >/dev/null || return $?
    unset "$url_name"
    attach_status=0
    docker start -a "$runner" || attach_status=$?
    status=$(docker inspect -f '{{.State.ExitCode}}' "$runner") || return 1
    [[ $status == 0 && $attach_status == 0 ]] || return 1
}
aggregate=0
summary=()
for stage in mysql postgres; do
    [[ $stages == all || $stages == "$stage" ]] || continue
    if run_stage; then
        summary+=("$stage PASS")
    else
        code=$?
        aggregate=1
        summary+=("$stage FAIL exit=$code (see per-driver results; setup errors leave drivers unrun)")
        echo "STAGE $stage FAIL exit=$code; cleaning before next stage" >&2
        if [[ $runner_attempted == 0 ]]; then
            for driver in sqlx diesel sync async; do
                [[ $features == "$stage-all" || $features == "$stage-$driver" ]] || continue
                echo "RESULT $stage/$driver BLOCKED (failure): server/stage setup failed before runner"
            done
        fi
        if [[ $server_attempted == 1 ]]; then docker logs "$server" >&2 || true; fi
    fi
    # Fully stop/remove this pair before starting the next database. Preserve
    # attempted flags until removal succeeds, so the EXIT trap retains mounts
    # on an uncertain or failed removal instead of deleting live data.
    for container in "$runner" "$server"; do
        attempted=$server_attempted
        [[ $container == "$runner" ]] && attempted=$runner_attempted
        [[ $attempted == 1 ]] || continue
        actual=$(docker inspect -f "{{index .Config.Labels \"$label_key\"}}" "$container")
        [[ $actual == "$run_id" ]] || { echo "Ownership changed: $container" >&2; exit 1; }
        docker rm -f "$container" >/dev/null
        if [[ $container == "$runner" ]]; then runner_attempted=0; else server_attempted=0; fi
    done
done
printf "SUMMARY %s\n" "${summary[@]}"
exit "$aggregate"
