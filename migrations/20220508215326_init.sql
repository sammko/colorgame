create table event (
    barcode    TEXT NOT NULL,
    station    INTEGER,
    timestamp  DATETIME NOT NULL,
    newcolor   INTEGER NOT NULL
);

create index idx_event_1 on event(barcode, timestamp desc);

create table color (
    barcode    TEXT PRIMARY KEY NOT NULL,
    color      INTEGER NOT NULL
);
