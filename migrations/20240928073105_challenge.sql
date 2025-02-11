-- Add migration script here
CREATE TABLE accounts (
    id SERIAL NOT NULL PRIMARY KEY,
    username VARCHAR(32) NOT NULL,
    avatar VARCHAR(256) NOT NULL
);

CREATE TABLE account_oauth_codes (
    id SERIAL NOT NULL PRIMARY KEY,
    account INTEGER NOT NULL REFERENCES accounts(id),
    access_token VARCHAR(64),
    refresh_token VARCHAR(64),
    id_on_provider BIGINT   
);

CREATE TABLE challenges (
    id SERIAL NOT NULL PRIMARY KEY,
    author INTEGER NOT NULL REFERENCES accounts(id),
    name varchar(255) NOT NULL,
    description TEXT NOT NULL,
    judge TEXT NOT NULL
);