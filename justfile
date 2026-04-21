release:
    cargo build --release

build:
    cargo build

test:
    @cargo test

fmt:
    @cargo fmt --all

clippy:
    @cargo clippy --fix --all-targets --allow-dirty -- -D warnings -W clippy::pedantic

check: fmt clippy test

major_upgrade:
    @cargo upgrade -i

minor_upgrade:
    @cargo upgrade

cargo_update:
    @cargo update

update: minor_upgrade cargo_update

upgrade: major_upgrade cargo_update
