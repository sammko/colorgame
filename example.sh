#!/bin/bash

set -e
source .env

[ ! -e config.json ] && cp config.example.json config.json
sqlx database create
sqlx migrate run
sqlx migrate info
