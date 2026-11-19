//! Phosphor icon names → codepoints (the same in Regular and Bold), like StarShell's
//! `Glyphs.js`. Only the icons Asteria uses are listed. To add one, find its name on
//! phosphoricons.com and its codepoint in `@phosphor-icons/web` 2.1.2's
//! `src/regular/style.css` (the bundled font's version).

pub const PLAY: &str = "\u{e3d0}";
pub const STOP: &str = "\u{e46c}";
pub const PLUS: &str = "\u{e3d4}";
pub const MINUS: &str = "\u{e32a}";
pub const X: &str = "\u{e4f6}";
pub const CHECK: &str = "\u{e182}";
pub const CARET_DOWN: &str = "\u{e136}";
pub const CARET_RIGHT: &str = "\u{e13a}";
pub const CARET_UP: &str = "\u{e13c}";
pub const CARET_LEFT: &str = "\u{e138}";
pub const ARROW_UP: &str = "\u{e08e}";
pub const ARROW_DOWN: &str = "\u{e03e}";
pub const ARROW_LEFT: &str = "\u{e058}";
pub const ARROW_RIGHT: &str = "\u{e06c}";
pub const ARROW_CLOCKWISE: &str = "\u{e036}";
pub const ARROW_COUNTER_CLOCKWISE: &str = "\u{e038}";
pub const FOLDER: &str = "\u{e24a}";
pub const FOLDER_OPEN: &str = "\u{e256}";
pub const GEAR: &str = "\u{e272}";
pub const TRASH: &str = "\u{e4a6}";
pub const PENCIL: &str = "\u{e3b4}";
pub const COPY: &str = "\u{e1ca}";
pub const DOWNLOAD: &str = "\u{e20c}";
pub const UPLOAD: &str = "\u{e4c0}";
pub const SEARCH: &str = "\u{e30c}";
pub const FUNNEL: &str = "\u{e266}";
pub const PACKAGE: &str = "\u{e390}";
pub const PUZZLE: &str = "\u{e596}";
pub const CUBE: &str = "\u{e1da}";
pub const GLOBE: &str = "\u{e288}";
pub const IMAGE: &str = "\u{e2ca}";
pub const IMAGES: &str = "\u{e836}";
pub const FILE_TEXT: &str = "\u{e23a}";
pub const FILE: &str = "\u{e230}";
pub const ARCHIVE: &str = "\u{e00c}";
pub const CLOCK: &str = "\u{e19a}";
pub const HISTORY: &str = "\u{e1a0}";
pub const WARNING: &str = "\u{e4e0}";
pub const INFO: &str = "\u{e2ce}";
pub const QUESTION: &str = "\u{e3e8}";
pub const USER: &str = "\u{e4c2}";
pub const USERS: &str = "\u{e4d6}";
pub const SIGN_IN: &str = "\u{e428}";
pub const SIGN_OUT: &str = "\u{e42a}";
pub const LINK: &str = "\u{e2e2}";
pub const LINK_BREAK: &str = "\u{e2e4}";
pub const SHARE: &str = "\u{e408}";
pub const LOCK: &str = "\u{e2fa}";
pub const KEYBOARD: &str = "\u{e2d8}";
pub const TERMINAL: &str = "\u{eae8}";
pub const BUG: &str = "\u{e5f4}";
pub const SLIDERS: &str = "\u{e434}";
pub const LIST: &str = "\u{e2f0}";
pub const DOTS: &str = "\u{e1fe}";
pub const GRIP: &str = "\u{eae2}";
pub const HARD_DRIVES: &str = "\u{e2a0}";
pub const NOTE: &str = "\u{e348}";
pub const SHIRT: &str = "\u{e670}";
pub const STACK: &str = "\u{e466}";
pub const SWAP: &str = "\u{e83c}";
pub const FLASK: &str = "\u{e79e}";

/// Every icon by name, for the kit gallery (and a test that each one is in the font).
pub const ALL: &[(&str, &str)] = &[
    ("play", PLAY),
    ("stop", STOP),
    ("plus", PLUS),
    ("minus", MINUS),
    ("x", X),
    ("check", CHECK),
    ("caret-down", CARET_DOWN),
    ("caret-right", CARET_RIGHT),
    ("caret-up", CARET_UP),
    ("caret-left", CARET_LEFT),
    ("arrow-up", ARROW_UP),
    ("arrow-down", ARROW_DOWN),
    ("arrow-left", ARROW_LEFT),
    ("arrow-right", ARROW_RIGHT),
    ("arrow-clockwise", ARROW_CLOCKWISE),
    ("arrow-counter-clockwise", ARROW_COUNTER_CLOCKWISE),
    ("folder", FOLDER),
    ("folder-open", FOLDER_OPEN),
    ("gear-six", GEAR),
    ("trash", TRASH),
    ("pencil-simple", PENCIL),
    ("copy", COPY),
    ("download-simple", DOWNLOAD),
    ("upload-simple", UPLOAD),
    ("magnifying-glass", SEARCH),
    ("funnel", FUNNEL),
    ("package", PACKAGE),
    ("puzzle-piece", PUZZLE),
    ("cube", CUBE),
    ("globe", GLOBE),
    ("image", IMAGE),
    ("images", IMAGES),
    ("file-text", FILE_TEXT),
    ("file", FILE),
    ("archive", ARCHIVE),
    ("clock", CLOCK),
    ("clock-counter-clockwise", HISTORY),
    ("warning", WARNING),
    ("info", INFO),
    ("question", QUESTION),
    ("user", USER),
    ("users", USERS),
    ("sign-in", SIGN_IN),
    ("sign-out", SIGN_OUT),
    ("link", LINK),
    ("link-break", LINK_BREAK),
    ("share-network", SHARE),
    ("lock", LOCK),
    ("keyboard", KEYBOARD),
    ("terminal-window", TERMINAL),
    ("bug", BUG),
    ("sliders-horizontal", SLIDERS),
    ("list", LIST),
    ("dots-three", DOTS),
    ("dots-six-vertical", GRIP),
    ("hard-drives", HARD_DRIVES),
    ("note", NOTE),
    ("t-shirt", SHIRT),
    ("stack", STACK),
    ("swap", SWAP),
    ("flask", FLASK),
];
