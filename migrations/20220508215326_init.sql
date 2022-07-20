create table event (
    barcode    TEXT NOT NULL,
    station    INTEGER,
    timestamp  DATETIME NOT NULL,
    newcolor   TEXT NOT NULL
);

create index idx_event_1 on event(barcode, timestamp desc);

create table color (
    barcode    TEXT PRIMARY KEY NOT NULL,
    station    INTEGER,
    color      TEXT NOT NULL
);

create table cycle_state (
    station    INTEGER PRIMARY KEY NOT NULL,
    last_index INTEGER NOT NULL DEFAULT 0
)