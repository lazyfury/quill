//! Backend-neutral text style values shared by the theme, the UI layer and the
//! render IR.

/// Weight of a text run, on the CSS/OpenType `usWeightClass` scale (1–1000,
/// conventionally 100–900).
///
/// A newtype rather than an enum so a font server can pick the *nearest*
/// available face: a family may ship 400/500/600 and no 700, and a request for
/// [`BOLD`](Self::BOLD) then resolves to 600. The common steps are named
/// constants; any value in between is valid via [`new`](Self::new).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontWeight(u16);

impl FontWeight {
    /// 100 — thin.
    pub const THIN: Self = Self(100);
    /// 200 — extra light.
    pub const EXTRA_LIGHT: Self = Self(200);
    /// 300 — light.
    pub const LIGHT: Self = Self(300);
    /// 400 — regular. The default.
    pub const NORMAL: Self = Self(400);
    /// 500 — medium.
    pub const MEDIUM: Self = Self(500);
    /// 600 — semibold.
    pub const SEMI_BOLD: Self = Self(600);
    /// 700 — bold.
    pub const BOLD: Self = Self(700);
    /// 800 — extra bold.
    pub const EXTRA_BOLD: Self = Self(800);
    /// 900 — black.
    pub const BLACK: Self = Self(900);

    /// A weight from a raw `usWeightClass` value.
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    /// The raw `usWeightClass` value.
    pub const fn value(self) -> u16 {
        self.0
    }

    /// Whether this weight is at least bold (700).
    pub const fn is_bold(self) -> bool {
        self.0 >= Self::BOLD.0
    }

    /// Whether this is the regular weight (400).
    pub const fn is_normal(self) -> bool {
        self.0 == Self::NORMAL.0
    }
}

impl Default for FontWeight {
    fn default() -> Self {
        Self::NORMAL
    }
}

impl From<u16> for FontWeight {
    fn from(value: u16) -> Self {
        Self(value)
    }
}

impl From<FontWeight> for u16 {
    fn from(weight: FontWeight) -> Self {
        weight.0
    }
}
