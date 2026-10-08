//! PNG color model and color-metadata handling support.
//!
//! This module contains the internal color model and color-metadata handling routines used by the PNG decoder.

use super::helpers::latin1_to_string;
use super::compression::decompress_zlib;

use remata_macros::FromPrimitive;

// ============================================================================
// PNG COLOR TYPE
// ============================================================================

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    FromPrimitive,
)]
/// Identifies the PNG color model encoded by the `IHDR` color-type field.
pub enum PngColorType {
    #[value(0)]
    /// Represents PNG grayscale samples with no alpha channel.
    Greyscale,

    #[value(2)]
    /// Represents PNG red, green, and blue samples with no alpha channel.
    Truecolor,

    #[value(3)]
    /// Represents PNG palette indices whose colors are supplied by a `PLTE` chunk.
    IndexedColor,

    #[value(4)]
    /// Represents PNG grayscale samples paired with alpha samples.
    GreyscaleWithAlpha,

    #[value(6)]
    /// Represents PNG red, green, blue, and alpha samples.
    TruecolorWithAlpha,
}

// ============================================================================
// sRGB
// ============================================================================

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    FromPrimitive,
)]
/// Identifies the standard sRGB rendering intent declared by PNG metadata.
pub enum Srgb {
    /// Perceptual rendering intent.
    #[value(0)]
    Perceptual,

    /// Relative colorimetric rendering intent.
    #[value(1)]
    RelativeColorimetric,

    /// Saturation rendering intent.
    #[value(2)]
    Saturation,

    /// Absolute colorimetric rendering intent.
    #[value(3)]
    AbsoluteColorimetric,
}

// ============================================================================
// TRANSPARENCY
// ============================================================================

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
)]
/// Represents transparency information parsed from a PNG `tRNS` chunk.
pub enum Transparency {
    /// Transparent grayscale sample.
    Greyscale(
        u16,
    ),

    /// Transparent RGB sample.
    Truecolor {
        red: u16,

        green: u16,

        blue: u16,
    },

    /// Alpha values corresponding to palette entries.
    IndexedColor(
        Vec<u8>,
    ),
}

impl Transparency {
    /// Parses the supplied encoded input into this format-specific representation.
    pub fn parse(
        color_type: PngColorType,
        data: &[u8],
    ) -> Option<Self> {
        match color_type {
            // ----------------------------------------------------------------
            // Grayscale
            // ----------------------------------------------------------------

            PngColorType::Greyscale => {
                if data.len() != 2 {
                    return None;
                }

                Some(
                    Self::Greyscale(
                        u16::from_be_bytes([
                            data[0],
                            data[1],
                        ])
                    )
                )
            }

            // ----------------------------------------------------------------
            // Truecolor
            // ----------------------------------------------------------------

            PngColorType::Truecolor => {
                if data.len() != 6 {
                    return None;
                }

                Some(
                    Self::Truecolor {
                        red:
                            u16::from_be_bytes([
                                data[0],
                                data[1],
                            ]),

                        green:
                            u16::from_be_bytes([
                                data[2],
                                data[3],
                            ]),

                        blue:
                            u16::from_be_bytes([
                                data[4],
                                data[5],
                            ]),
                    }
                )
            }

            // ----------------------------------------------------------------
            // Indexed
            // ----------------------------------------------------------------

            PngColorType::IndexedColor => {
                if data.is_empty() {
                    return None;
                }

                Some(
                    Self::IndexedColor(
                        data.to_vec()
                    )
                )
            }

            // ----------------------------------------------------------------
            // Alpha color types cannot contain tRNS.
            // ----------------------------------------------------------------

            PngColorType::GreyscaleWithAlpha
            | PngColorType::TruecolorWithAlpha => {
                None
            }
        }
    }
}

// ============================================================================
// SIGNIFICANT BITS
// ============================================================================

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
)]
/// Stores the significant-bit declaration appropriate to a PNG color type.
pub enum SignificantBits {
    /// Stores the number of significant bits used by grayscale samples.
    Greyscale {
        greyscale: u8,
    },

    /// Stores significant-bit counts for red, green, and blue truecolor samples.
    Truecolor {
        red: u8,
        green: u8,
        blue: u8,
    },

    /// Stores significant-bit counts for the red, green, and blue components of palette entries.
    IndexedColor {
        red: u8,
        green: u8,
        blue: u8,
    },

    /// Stores significant-bit counts for grayscale and alpha samples.
    GreyscaleWithAlpha {
        greyscale: u8,
        alpha: u8,
    },

    /// Stores significant-bit counts for red, green, blue, and alpha samples.
    TruecolorWithAlpha {
        red: u8,
        green: u8,
        blue: u8,
        alpha: u8,
    },
}

impl SignificantBits {
    /// Parses the supplied encoded input into this format-specific representation.
    pub fn parse(
        color_type: PngColorType,
        data: &[u8],
    ) -> Option<Self> {
        match color_type {
            PngColorType::Greyscale => {
                if data.len() != 1 {
                    return None;
                }

                Some(
                    Self::Greyscale {
                        greyscale:
                            data[0],
                    }
                )
            }

            PngColorType::Truecolor => {
                if data.len() != 3 {
                    return None;
                }

                Some(
                    Self::Truecolor {
                        red: data[0],
                        green: data[1],
                        blue: data[2],
                    }
                )
            }

            PngColorType::IndexedColor => {
                if data.len() != 3 {
                    return None;
                }

                Some(
                    Self::IndexedColor {
                        red: data[0],
                        green: data[1],
                        blue: data[2],
                    }
                )
            }

            PngColorType::GreyscaleWithAlpha => {
                if data.len() != 2 {
                    return None;
                }

                Some(
                    Self::GreyscaleWithAlpha {
                        greyscale:
                            data[0],

                        alpha:
                            data[1],
                    }
                )
            }

            PngColorType::TruecolorWithAlpha => {
                if data.len() != 4 {
                    return None;
                }

                Some(
                    Self::TruecolorWithAlpha {
                        red: data[0],
                        green: data[1],
                        blue: data[2],
                        alpha: data[3],
                    }
                )
            }
        }
    }
}

// ============================================================================
// CHROMATICITY
// ============================================================================

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
)]
/// Stores PNG primary-color and white-point chromaticity coordinates.
pub struct Chromaticity {
    /// Stores the scaled X chromaticity coordinate of the reference white point.
    pub white_x: u32,

    /// Stores the scaled Y chromaticity coordinate of the reference white point.
    pub white_y: u32,

    /// Stores the scaled X chromaticity coordinate of the red primary.
    pub red_x: u32,

    /// Stores the scaled Y chromaticity coordinate of the red primary.
    pub red_y: u32,

    /// Stores the scaled X chromaticity coordinate of the green primary.
    pub green_x: u32,

    /// Stores the scaled Y chromaticity coordinate of the green primary.
    pub green_y: u32,

    /// Stores the scaled X chromaticity coordinate of the blue primary.
    pub blue_x: u32,

    /// Stores the scaled Y chromaticity coordinate of the blue primary.
    pub blue_y: u32,
}

impl Chromaticity {
    /// Parses the supplied encoded input into this format-specific representation.
    pub fn parse(
        data: &[u8],
    ) -> Option<Self> {
        if data.len() != 32 {
            return None;
        }

        Some(Self {
            white_x:
                read_u32(
                    data,
                    0,
                ),

            white_y:
                read_u32(
                    data,
                    4,
                ),

            red_x:
                read_u32(
                    data,
                    8,
                ),

            red_y:
                read_u32(
                    data,
                    12,
                ),

            green_x:
                read_u32(
                    data,
                    16,
                ),

            green_y:
                read_u32(
                    data,
                    20,
                ),

            blue_x:
                read_u32(
                    data,
                    24,
                ),

            blue_y:
                read_u32(
                    data,
                    28,
                ),
        })
    }
}

// ============================================================================
// CONTENT LIGHT LEVEL
// ============================================================================

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
)]
/// Stores content light-level metadata associated with HDR image data.
pub struct ContentLightLevel {
    /// Stores the maximum content light level declared by HDR metadata.
    pub maximum_content_light_level:
        u32,

    /// Stores the maximum frame-average light level declared by HDR metadata.
    pub maximum_frame_average_light_level:
        u32,
}

impl ContentLightLevel {
    /// Parses the supplied encoded input into this format-specific representation.
    pub fn parse(
        data: &[u8],
    ) -> Option<Self> {
        if data.len() != 8 {
            return None;
        }

        Some(Self {
            maximum_content_light_level:
                read_u32(
                    data,
                    0,
                ),

            maximum_frame_average_light_level:
                read_u32(
                    data,
                    4,
                ),
        })
    }
}

// ============================================================================
// ICC PROFILE
// ============================================================================

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
)]
/// Stores a named ICC color profile embedded in PNG metadata.
pub struct IccProfile {
    /// Profile name from the iCCP chunk.
    pub name: String,

    /// Decompressed ICC profile bytes.
    pub profile: Vec<u8>,
}

impl IccProfile {
    /// Parse an iCCP chunk.
    ///
    /// Layout:
    ///
    /// ```text
    /// profile name
    /// 0
    /// compression method
    /// compressed profile
    /// ```
    pub fn parse(
        data: &[u8],
    ) -> Option<Self> {
        // --------------------------------------------------------------------
        // Profile name
        // --------------------------------------------------------------------

        let separator =
            data.iter()
                .position(
                    |&byte| {
                        byte == 0
                    }
                )?;

        if separator == 0
            || separator > 79
        {
            return None;
        }

        let compression_index =
            separator
                .checked_add(1)?;

        if compression_index
            >= data.len()
        {
            return None;
        }

        // --------------------------------------------------------------------
        // Compression method
        // --------------------------------------------------------------------

        // PNG currently defines only compression method 0, which means
        // zlib/DEFLATE.
        if data[
            compression_index
        ] != 0
        {
            return None;
        }

        let compressed_start =
            compression_index
                .checked_add(1)?;

        if compressed_start
            > data.len()
        {
            return None;
        }

        // --------------------------------------------------------------------
        // Profile name
        // --------------------------------------------------------------------

        let name =
            latin1_to_string(
                &data[
                    ..separator
                ]
            );

        // --------------------------------------------------------------------
        // ICC profile
        // --------------------------------------------------------------------

        let profile =
            decompress_zlib(
                &data[
                    compressed_start..
                ]
            )?;

        Some(Self {
            name,
            profile,
        })
    }
}
// ============================================================================
// HELPERS
// ============================================================================

#[inline(always)]
/// Reads u32 from the supplied input while advancing the relevant cursor or bit position.
fn read_u32(
    data: &[u8],
    offset: usize,
) -> u32 {
    u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}
