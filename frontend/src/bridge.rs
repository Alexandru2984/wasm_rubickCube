//! Page ⇄ engine bridge.
//!
//! The UI is plain HTML/CSS/JS. It drives the engine through a single
//! exported function, `cube_cmd(name, arg)`, and listens to `cube:*` DOM
//! events that the engine dispatches on `window`.
//!
//! Everything arriving from the page is treated as untrusted: names are
//! matched against a fixed list, arguments are length-capped and parsed with
//! bounds, and the inbox itself is bounded so a runaway caller can't grow it.

use std::cell::RefCell;
use std::collections::VecDeque;

const MAX_ARG_LEN: usize = 2048;
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const MAX_INBOX: usize = 128;

#[derive(Debug, Clone, PartialEq)]
pub enum PlayerOp {
    Play,
    Pause,
    Next,
    Prev,
    Start,
    End,
    Close,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiCommand {
    Hello,
    Size(i32),
    /// Random scramble; `seed` for reproducible ones, `daily` tags the daily challenge.
    Scramble {
        seed: Option<u64>,
        daily: Option<u32>,
    },
    ScrambleAlg(String),
    Solve {
        autoplay: bool,
    },
    Rewind,
    Undo,
    Redo,
    Reset,
    Move(String),
    PlayAlg(String),
    Setup {
        alg: String,
        inverse: bool,
    },
    Trainer {
        case: usize,
        auf: bool,
    },
    Player(PlayerOp),
    ViewReset,
    Settings(String),
    /// Ask for the current history as notation (for share links).
    Export,
}

impl UiCommand {
    pub fn parse(name: &str, arg: &str) -> Option<Self> {
        if arg.len() > MAX_ARG_LEN {
            return None;
        }
        let arg = arg.trim();
        Some(match name {
            "hello" => Self::Hello,
            "size" => Self::Size(
                arg.parse::<i32>().ok().filter(|n| (crate::cube::MIN_SIZE..=crate::cube::MAX_SIZE).contains(n))?,
            ),
            "scramble" => {
                if arg.is_empty() {
                    Self::Scramble { seed: None, daily: None }
                } else if let Some(s) = arg.strip_prefix("seed:") {
                    Self::Scramble { seed: Some(s.parse().ok()?), daily: None }
                } else {
                    let day: u32 = arg.strip_prefix("daily:")?.parse().ok()?;
                    Self::Scramble { seed: Some(daily_seed(day)), daily: Some(day) }
                }
            }
            "scramble_alg" => Self::ScrambleAlg(arg.to_owned()),
            "solve" => Self::Solve { autoplay: arg != "hint" },
            "rewind" => Self::Rewind,
            "undo" => Self::Undo,
            "redo" => Self::Redo,
            "reset" => Self::Reset,
            "move" => Self::Move(arg.to_owned()),
            "play_alg" => Self::PlayAlg(arg.to_owned()),
            "setup" => Self::Setup { alg: arg.to_owned(), inverse: false },
            "setup_inverse" => Self::Setup { alg: arg.to_owned(), inverse: true },
            "trainer" => {
                let (idx, auf) = arg.split_once(',').unwrap_or((arg, "0"));
                let case: usize = idx.parse().ok().filter(|&i| i < crate::trainer::ALGS.len())?;
                Self::Trainer { case, auf: auf == "1" }
            }
            "player" => Self::Player(match arg {
                "play" => PlayerOp::Play,
                "pause" => PlayerOp::Pause,
                "next" => PlayerOp::Next,
                "prev" => PlayerOp::Prev,
                "start" => PlayerOp::Start,
                "end" => PlayerOp::End,
                "close" => PlayerOp::Close,
                _ => return None,
            }),
            "view_reset" => Self::ViewReset,
            "settings" => Self::Settings(arg.to_owned()),
            "export" => Self::Export,
            _ => return None,
        })
    }
}

/// Daily challenge seed: a fixed mix of the day number, identical for every
/// visitor on that UTC day.
pub fn daily_seed(day: u32) -> u64 {
    (day as u64).wrapping_mul(0x2545_F491_4F6C_DD1D) ^ 0xC0BE_D411_7000_0000
}

thread_local! {
    static INBOX: RefCell<VecDeque<UiCommand>> = const { RefCell::new(VecDeque::new()) };
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn push(cmd: UiCommand) {
    INBOX.with(|q| {
        let mut q = q.borrow_mut();
        if q.len() < MAX_INBOX {
            q.push_back(cmd);
        }
    });
}

pub fn drain() -> Vec<UiCommand> {
    INBOX.with(|q| q.borrow_mut().drain(..).collect())
}

// ── wasm side ─────────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
mod web {
    use super::*;
    use bevy::winit::{EventLoopProxy, WakeUp};
    use wasm_bindgen::prelude::*;

    thread_local! {
        static WAKER: RefCell<Option<EventLoopProxy<WakeUp>>> = const { RefCell::new(None) };
    }

    pub fn set_waker(proxy: EventLoopProxy<WakeUp>) {
        WAKER.with(|w| *w.borrow_mut() = Some(proxy));
    }

    /// The single entry point from the page. Returns false for unknown or
    /// malformed commands (the page logs those; nothing is queued).
    #[wasm_bindgen]
    pub fn cube_cmd(name: &str, arg: &str) -> bool {
        let Some(cmd) = UiCommand::parse(name, arg) else {
            return false;
        };
        push(cmd);
        // The engine sleeps while idle; wake it so the command runs now.
        WAKER.with(|w| {
            if let Some(p) = w.borrow().as_ref() {
                let _ = p.send_event(WakeUp);
            }
        });
        true
    }

    pub fn emit(kind: &str, json: &str) {
        let Some(win) = web_sys::window() else { return };
        let init = web_sys::CustomEventInit::new();
        init.set_detail(&JsValue::from_str(json));
        if let Ok(ev) = web_sys::CustomEvent::new_with_event_init_dict(&format!("cube:{kind}"), &init) {
            let _ = win.dispatch_event(&ev);
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use web::{emit, set_waker};

#[cfg(not(target_arch = "wasm32"))]
pub fn emit(_kind: &str, _json: &str) {}

// ── Tiny JSON writer (all payloads are flat and engine-generated) ─────────────

pub fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            // Keep the payload safe even if a consumer ever injects it as HTML.
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub struct Json(String);

impl Json {
    pub fn new() -> Self {
        Json(String::from("{"))
    }
    fn key(&mut self, k: &str) {
        if self.0.len() > 1 {
            self.0.push(',');
        }
        self.0.push_str(&json_str(k));
        self.0.push(':');
    }
    pub fn num(mut self, k: &str, v: impl std::fmt::Display) -> Self {
        self.key(k);
        self.0.push_str(&v.to_string());
        self
    }
    pub fn float(mut self, k: &str, v: f64) -> Self {
        self.key(k);
        if v.is_finite() {
            self.0.push_str(&format!("{v:.3}"));
        } else {
            self.0.push_str("null");
        }
        self
    }
    pub fn bool(mut self, k: &str, v: bool) -> Self {
        self.key(k);
        self.0.push_str(if v { "true" } else { "false" });
        self
    }
    pub fn str(mut self, k: &str, v: &str) -> Self {
        self.key(k);
        self.0.push_str(&json_str(v));
        self
    }
    pub fn raw(mut self, k: &str, v: &str) -> Self {
        self.key(k);
        self.0.push_str(v);
        self
    }
    pub fn done(mut self) -> String {
        self.0.push('}');
        self.0
    }
}

pub fn json_array<I: IntoIterator<Item = String>>(items: I) -> String {
    let v: Vec<String> = items.into_iter().collect();
    format!("[{}]", v.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_commands_only() {
        assert_eq!(UiCommand::parse("size", "5"), Some(UiCommand::Size(5)));
        assert_eq!(UiCommand::parse("size", "21"), None);
        assert_eq!(UiCommand::parse("size", "-1"), None);
        assert_eq!(UiCommand::parse("size", "abc"), None);
        assert_eq!(UiCommand::parse("trainer", "9999"), None);
        assert_eq!(UiCommand::parse("trainer", "3,1"), Some(UiCommand::Trainer { case: 3, auf: true }));
        assert_eq!(UiCommand::parse("scramble", "seed:42"), Some(UiCommand::Scramble { seed: Some(42), daily: None }));
        assert_eq!(UiCommand::parse("scramble", "daily:x"), None);
        assert_eq!(UiCommand::parse("scramble", "bogus"), None);
        assert_eq!(UiCommand::parse("player", "explode"), None);
        assert_eq!(UiCommand::parse("__proto__", ""), None);
        assert_eq!(UiCommand::parse("move", &"R".repeat(5000)), None);
    }

    #[test]
    fn inbox_is_bounded() {
        for _ in 0..1000 {
            push(UiCommand::Hello);
        }
        assert_eq!(drain().len(), MAX_INBOX);
        assert!(drain().is_empty());
    }

    #[test]
    fn daily_seed_is_stable() {
        assert_eq!(daily_seed(20_000), daily_seed(20_000));
        assert_ne!(daily_seed(20_000), daily_seed(20_001));
    }

    #[test]
    fn json_escapes() {
        assert_eq!(json_str("a\"b\\c<d>"), "\"a\\\"b\\\\c\\u003cd\\u003e\"");
        let j = Json::new().num("n", 3).bool("b", true).str("s", "x").float("f", f64::NAN).done();
        assert_eq!(j, "{\"n\":3,\"b\":true,\"s\":\"x\",\"f\":null}");
    }
}
