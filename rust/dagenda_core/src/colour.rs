//! A track's colour: one of the picture's palette, by name, or any colour as
//! `#rrggbb`.
//!
//! The palette's eight colours are tuned by hand for light and dark themes.
//! Any other colour gets tints derived the same way: pale fills and a dark
//! heading on a light page, deep fills and a pale heading on a dark one.
//! Names and hex digits are read in any case.
//!
//! ```
//! use dagenda_core::colour::{Colour, Named};
//!
//! assert_eq!(Colour::new("plum")?, Colour::Named(Named::Plum));
//! assert_eq!(Colour::new("#2A9D8F")?.to_string(), "#2a9d8f");
//! assert!(Colour::new("red").is_err());
//! # Ok::<(), dagenda_core::colour::InvalidColour>(())
//! ```

use alloc::string::{String, ToString};
use core::fmt;
use thiserror::Error;

/// A track's colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Colour {
    /// One of the palette's.
    Named(Named),

    /// Any colour, by its red, green, and blue.
    Rgb(Rgb),
}

impl Colour {
    /// Read a colour: a palette name, or `#` and six hex digits.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidColour`] for anything else.
    pub fn new(written: &str) -> Result<Self, InvalidColour> {
        if let Some(hex) = written.strip_prefix('#') {
            return Rgb::from_hex(hex).map(Self::Rgb);
        }
        Named::ALL
            .into_iter()
            .find(|named| named.name().eq_ignore_ascii_case(written))
            .map(Self::Named)
            .ok_or_else(|| InvalidColour::Unknown(written.to_string()))
    }
}

/// As written, in canonical form: a lowercase name, or `#rrggbb`.
impl fmt::Display for Colour {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Named(named) => f.write_str(named.name()),
            Self::Rgb(rgb) => rgb.fmt(f),
        }
    }
}

/// The palette's colours, in the order tracks are given them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Named {
    /// Teal.
    Teal,
    /// Indigo.
    Indigo,
    /// Rose.
    Rose,
    /// Ochre.
    Ochre,
    /// Sage.
    Sage,
    /// Plum.
    Plum,
    /// Sky.
    Sky,
    /// Terracotta.
    Terracotta,
}

impl Named {
    /// Every colour in the palette, in order.
    pub const ALL: [Self; 8] = [
        Self::Teal,
        Self::Indigo,
        Self::Rose,
        Self::Ochre,
        Self::Sage,
        Self::Plum,
        Self::Sky,
        Self::Terracotta,
    ];

    /// Its name, as written.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Teal => "teal",
            Self::Indigo => "indigo",
            Self::Rose => "rose",
            Self::Ochre => "ochre",
            Self::Sage => "sage",
            Self::Plum => "plum",
            Self::Sky => "sky",
            Self::Terracotta => "terracotta",
        }
    }

    /// Where it is in [`Named::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// A colour by its red, green, and blue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb([u8; 3]);

impl Rgb {
    /// A colour from its channels.
    #[must_use]
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self([red, green, blue])
    }

    /// Its red, green, and blue.
    #[must_use]
    pub const fn channels(self) -> [u8; 3] {
        self.0
    }

    /// This colour, `thousandths` of the way to `other`.
    ///
    /// ```
    /// use dagenda_core::colour::Rgb;
    ///
    /// let teal = Rgb::new(42, 157, 143);
    /// assert_eq!(teal.mix(Rgb::new(255, 255, 255), 0).to_string(), "#2a9d8f");
    /// assert_eq!(teal.mix(Rgb::new(255, 255, 255), 1000).to_string(), "#ffffff");
    /// assert_eq!(teal.mix(Rgb::new(255, 255, 255), 700).to_string(), "#bfe2dd");
    /// ```
    #[must_use]
    pub fn mix(self, other: Self, thousandths: u16) -> Self {
        let part = u32::from(thousandths.min(1000));
        let channel = |mine: u8, theirs: u8| {
            let mixed = (u32::from(mine) * (1000 - part) + u32::from(theirs) * part + 500) / 1000;
            u8::try_from(mixed).unwrap_or(u8::MAX)
        };
        let ([r, g, b], [or, og, ob]) = (self.0, other.0);
        Self([channel(r, or), channel(g, og), channel(b, ob)])
    }

    fn from_hex(hex: &str) -> Result<Self, InvalidColour> {
        let digits = hex.as_bytes();
        if digits.len() != 6 || !digits.iter().all(u8::is_ascii_hexdigit) {
            return Err(InvalidColour::Hex);
        }
        let pair = |at: usize| {
            hex.get(at..at + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .ok_or(InvalidColour::Hex)
        };
        Ok(Self([pair(0)?, pair(2)?, pair(4)?]))
    }
}

/// `#rrggbb`, in lowercase.
impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [red, green, blue] = self.0;
        write!(f, "#{red:02x}{green:02x}{blue:02x}")
    }
}

/// Why some text isn't a colour.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InvalidColour {
    /// A name that isn't in the palette.
    #[error(
        "there's no colour `{0}`; use teal, indigo, rose, ochre, sage, plum, sky, or terracotta, or a hex colour like `#2a9d8f`"
    )]
    Unknown(String),

    /// A `#` without exactly six hex digits after it.
    #[error("a hex colour is `#` and six hex digits, as in `#2a9d8f`")]
    Hex,
}
