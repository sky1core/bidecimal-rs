# Live database integration tests

Live-server tests are isolated in the `bidecimal-db-integration` workspace
member, outside default `cargo test`.

## Run with Docker

On Linux or macOS, amd64 or arm64, from the repository root:

```sh
bash scripts/db-integration.sh           # MySQL, then PostgreSQL
bash scripts/db-integration.sh mysql
bash scripts/db-integration.sh postgres
```

The runner uses disposable, uniquely labeled containers and a network, with no
published host port. Each container has a one-CPU quota; each
database/runner pair has two CPUs total. Source is a
read-only bind; database data/tmp use run-specific binds. Cargo cache and target
use each runner's writable layer, so they are not reused between database stages.
Cleanup removes only this run's resources; images are retained. Docker must have
sufficient free VM disk space for both native dependencies and Rust build output.

## Environment variables

- `BIDECIMAL_MYSQL_TEST_FEATURES`: `mysql-all` (default), `mysql-sqlx`,
  `mysql-diesel`, `mysql-sync`, or `mysql-async`.
- `BIDECIMAL_POSTGRES_TEST_FEATURES`: `postgres-all` (default), `postgres-sqlx`,
  `postgres-diesel`, `postgres-sync`, or `postgres-async` (tokio-postgres).
- `TMPDIR`: optional host parent for the run's temporary bind directories.

The Docker runner sets database URLs automatically. To use an already provisioned
**disposable** database, set the corresponding URL and run directly:

```sh
export BIDECIMAL_TEST_MYSQL_URL='<mysql:// URL of the disposable database>'
cargo test --locked -p bidecimal-db-integration --features mysql-all --test mysql -- --test-threads=1 --nocapture
export BIDECIMAL_TEST_POSTGRES_URL='<postgres:// URL of the disposable database>'
cargo test --locked -p bidecimal-db-integration --features postgres-all --test postgres -- --test-threads=1 --nocapture
```

URLs are mandatory for selected live tests; missing URLs fail rather than skip.
Tests create temporary tables and change session settings. Do not use production
databases. Direct Diesel builds require the corresponding native client
libraries. For compile-only checks, replace the final test arguments with
`--no-run`; this does not execute SQL or establish live-test success.

## Pinned versions

The live suites first assert MySQL 9.7.4 or PostgreSQL 18.6, respectively,
as provided by the Docker script. A different server version fails this first
check by design, including when using a directly configured database URL.

The Docker script pins these image digests:

- MySQL 9.7.4: `sha256:df90258da46f6e7ff8574ff84f17618aba0765cac749928299c69fd4e7cb8ad8`.
- PostgreSQL 18.6: `sha256:3a4264520e121176619f54ff867d12774bc81e95e7b84db58d63bccdb6144b81`.
- Rust 1.98.0 bookworm: `sha256:82150a52ec202c1b14d7817e14516c392bb7f5cfebd88f1ed531cb37ebd39922`.

`RUSTUP_TOOLCHAIN=1.98.0` overrides the development toolchain in the runner;
Cargo uses the checked-in lockfile. `install-native-client.sh` builds MariaDB
Connector/C 3.3.21 with OpenSSL into a disposable private prefix, from commit
`ead038dc8a2b3d7819f87af33e14f105325f8cf9`, archive SHA-256
`10e973b462e2ba0b003b9d7d707b2aee89be13c4654de79d8ba6a53f348263aa`.
Build prerequisites and libpq come from official Debian apt repositories; their
package versions are not snapshot-pinned. No host native-client install is needed
when using the Docker runner.

With Debian Bookworm libmariadb3 10.11.18 / Connector/C 3.3.19, CONC-821 can
produce empty DECIMAL results during native rebinding; the Diesel adapter
rejects these as `InvalidInput` rather than converting them to zero. The Docker
runner uses the pinned Connector/C 3.3.21 above.
