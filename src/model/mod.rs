use crate::prelude::*;

pub(crate) mod ask;
pub(crate) mod chat_mode;
pub(crate) mod commands;
pub(crate) mod constants;
pub(crate) mod editors;
pub(crate) mod effort;
pub(crate) mod language;
pub(crate) mod mode;
pub(crate) mod overlay;
pub(crate) mod plugin;
pub(crate) mod provider;
pub(crate) mod run_access;
pub(crate) mod shortcuts;
pub(crate) mod theme;
pub(crate) mod usage;

pub(crate) use ask::*;
pub(crate) use chat_mode::*;
pub(crate) use commands::*;
pub(crate) use constants::*;
pub(crate) use editors::*;
pub(crate) use effort::*;
pub(crate) use language::*;
pub(crate) use mode::*;
pub(crate) use overlay::*;
pub(crate) use plugin::*;
pub(crate) use provider::*;
pub(crate) use run_access::*;
pub(crate) use shortcuts::*;
pub(crate) use theme::*;
pub(crate) use usage::*;

pub(crate) type AnyResult<T> = Result<T, Box<dyn Error>>;
