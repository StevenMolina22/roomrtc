#!/bin/bash

cargo fmt

cargo clippy --fix --allow-dirty --allow-staged --all-features --all-targets -- -W clippy::all -W clippy::pedantic -W clippy::nursery
cargo clippy --all-features --all-targets -- -W clippy::all -W clippy::pedantic -W clippy::nursery
