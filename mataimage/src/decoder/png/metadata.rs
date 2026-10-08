//! PNG metadata chunk parsing support.
//!
//! This module contains the internal metadata chunk parsing routines used by the PNG decoder.

use remata_macros::FromPrimitive;

// ============================================================================
// PLTE
// ============================================================================

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
)]
/// Stores the palette entries parsed from a PNG `PLTE` chunk.
pub struct Plte {
    /// Stores the entries belonging to this directory or table.
    pub entries:
        Vec<(u8, u8, u8)>,
}

impl Plte {
    #[inline]
    /// Parses the supplied encoded input into this format-specific representation.
    pub fn parse(
        data: &[u8],
    ) -> Option<Self> {
        if data.is_empty()
            || data.len() % 3 != 0
        {
            return None;
        }

        // PNG palettes contain at most 256 entries.
        if data.len() > 256 * 3 {
            return None;
        }

        let mut entries =
            Vec::with_capacity(
                data.len() / 3
            );

        for chunk in
            data.chunks_exact(3)
        {
            entries.push((
                chunk[0],
                chunk[1],
                chunk[2],
            ));
        }

        Some(Self {
            entries,
        })
    }
}

// ============================================================================
// PHYSICAL
// ============================================================================

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
)]
/// Stores PNG physical pixel-dimension metadata from the `pHYs` chunk.
pub struct Physical {
    /// Stores the horizontal pixel density declared by the PNG `pHYs` chunk.
    pub x: u32,

    /// Stores the vertical pixel density declared by the PNG `pHYs` chunk.
    pub y: u32,

    /// Stores the unit used to interpret the horizontal and vertical PNG pixel densities.
    pub unit: PhysicalUnit,
}

impl Physical {
    #[inline]
    /// Parses the supplied encoded input into this format-specific representation.
    pub fn parse(
        data: &[u8],
    ) -> Option<Self> {
        if data.len() != 9 {
            return None;
        }

        let x =
            u32::from_be_bytes([
                data[0],
                data[1],
                data[2],
                data[3],
            ]);

        let y =
            u32::from_be_bytes([
                data[4],
                data[5],
                data[6],
                data[7],
            ]);

        let unit =
            PhysicalUnit::try_from(
                data[8]
            )
            .ok()?;

        Some(Self {
            x,
            y,
            unit,
        })
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    FromPrimitive,
)]
/// Identifies the unit associated with PNG physical pixel dimensions.
pub enum PhysicalUnit {
    #[value(1)]
    /// Indicates that PNG physical pixel densities are measured in pixels per metre.
    Meter,

    #[value(0)]
    /// Indicates that PNG physical pixel-density values have an unspecified unit.
    Unknown,
}

// ============================================================================
// TIME
// ============================================================================

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
)]
/// Stores the image modification timestamp parsed from a PNG `tIME` chunk.
pub struct Time {
    /// Stores the four-digit UTC modification year from the PNG `tIME` chunk.
    pub year: u16,

    /// Stores the UTC modification month from the PNG `tIME` chunk.
    pub month: u8,

    /// Stores the UTC modification day of month from the PNG `tIME` chunk.
    pub day: u8,

    /// Stores the UTC modification hour from the PNG `tIME` chunk.
    pub hour: u8,

    /// Stores the UTC modification minute from the PNG `tIME` chunk.
    pub minute: u8,

    /// Stores the UTC modification second from the PNG `tIME` chunk.
    pub second: u8,
}

impl Time {
    #[inline]
    /// Parses the supplied encoded input into this format-specific representation.
    pub fn parse(
        data: &[u8],
    ) -> Option<Self> {
        if data.len() != 7 {
            return None;
        }

        let value =
            Self {
                year:
                    u16::from_be_bytes([
                        data[0],
                        data[1],
                    ]),

                month: data[2],

                day: data[3],

                hour: data[4],

                minute: data[5],

                second: data[6],
            };

        // Basic tIME range validation.
        if !(1..=12)
            .contains(
                &value.month
            )
        {
            return None;
        }

        if !(1..=31)
            .contains(
                &value.day
            )
        {
            return None;
        }

        if value.hour > 23
            || value.minute > 59
            || value.second > 60
        {
            return None;
        }

        Some(value)
    }
}
