Usage
---

The first thing that has to be done is installation of `sqlx`. This is done by running `cargo install sqlx-cli`.
The `sqlx` crate requires access to the database at compilation time, so create
one before running `cargo build`. A script ([example.sh](./example.sh)) is
provided which creates and migrates
the database configured in [.env](./.env) (`data.db` by default).

A configuration file named `config.json` is required to start the server.
An example is provided in [config.json.example](./config.json.example). This
file is copied to the correct location by the example.sh script if one does
not already exist.

Once built, `cargo run` can be used to start the server. The `.env` file is
respected.

Summary:

- `cargo install sqlx-cli`
- `./example.sh` (create config and database)
- `cargo build`
- `cargo run`
