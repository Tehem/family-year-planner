//! Family Year Planner server: web app, JSON API, authentication, MCP and imports.

pub mod admin;
pub mod api;
pub mod app;
pub mod bootstrap;
pub mod cli;
pub mod config;
pub mod crypto;
pub mod dates_fr;
pub mod error;
pub mod google_login;
pub mod imports;
pub mod kiosk;
pub mod mcp;
pub mod oauth;
pub mod pages;
pub mod ratelimit;
pub mod security;
pub mod service;
pub mod session;
pub mod state;
pub mod storage;
pub mod tokens;
pub mod web;
