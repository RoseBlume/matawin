//! PNG text metadata parsing support.
//!
//! This module contains the internal text metadata parsing routines used by the PNG decoder.

use super::helpers::latin1_to_string;
use super::compression::decompress_zlib;
// ============================================================================
// tEXt
// ============================================================================

/// Parse an uncompressed PNG tEXt chunk.
///
/// The returned representation remains compatible with the original decoder:
///
/// ```text
/// keyword\0text
/// ```
pub(crate) fn parse_text(
    data: &[u8],
) -> Option<String> {
    let separator =
        find_null(
            data,
            0,
        )?;

    if separator == 0
        || separator > 79
    {
        return None;
    }

    let keyword =
        latin1_to_string(
            &data[..separator]
        );

    let text =
        latin1_to_string(
            &data[
                separator + 1..
            ]
        );

    Some(
        combine_keyword_text(
            &keyword,
            &text,
        )
    )
}

// ============================================================================
// zTXt
// ============================================================================

/// Decode a compressed PNG zTXt chunk.
///
/// Layout:
///
/// ```text
/// keyword
/// 0
/// compression method
/// compressed text
/// ```
pub fn decompress_ztxt(
    data: &[u8],
) -> Option<String> {
    let separator =
        find_null(
            data,
            0,
        )?;

    if separator == 0
        || separator > 79
    {
        return None;
    }

    let compression_method_index =
        separator + 1;

    if compression_method_index
        >= data.len()
    {
        return None;
    }

    // PNG defines compression method 0 for zTXt.
    if data[
        compression_method_index
    ] != 0
    {
        return None;
    }

    let compressed_start =
        compression_method_index + 1;

    let compressed =
        &data[
            compressed_start..
        ];

    let decompressed =
        decompress_zlib(
            compressed
        )?;

    let keyword =
        latin1_to_string(
            &data[..separator]
        );

    let text =
        latin1_to_string(
            &decompressed
        );

    Some(
        combine_keyword_text(
            &keyword,
            &text,
        )
    )
}

// ============================================================================
// iTXt
// ============================================================================

/// Parse an international PNG iTXt chunk.
///
/// The returned representation remains:
///
/// ```text
/// keyword\0text
/// ```
pub(crate) fn parse_itxt(
    data: &[u8],
) -> Option<String> {
    // ------------------------------------------------------------------------
    // Keyword
    // ------------------------------------------------------------------------

    let keyword_end =
        find_null(
            data,
            0,
        )?;

    if keyword_end == 0
        || keyword_end > 79
    {
        return None;
    }

    let keyword =
        latin1_to_string(
            &data[
                ..keyword_end
            ]
        );

    let mut offset =
        keyword_end + 1;

    // ------------------------------------------------------------------------
    // Compression flag + compression method
    // ------------------------------------------------------------------------

    if offset + 2
        > data.len()
    {
        return None;
    }

    let compression_flag =
        data[offset];

    let compression_method =
        data[offset + 1];

    offset += 2;

    if compression_flag > 1 {
        return None;
    }

    if compression_flag == 1
        && compression_method != 0
    {
        return None;
    }

    // ------------------------------------------------------------------------
    // Language tag
    // ------------------------------------------------------------------------

    let language_end =
        find_null(
            data,
            offset,
        )?;

    offset =
        language_end + 1;

    // ------------------------------------------------------------------------
    // Translated keyword
    // ------------------------------------------------------------------------

    let translated_end =
        find_null(
            data,
            offset,
        )?;

    offset =
        translated_end + 1;

    // ------------------------------------------------------------------------
    // Text
    // ------------------------------------------------------------------------

    let text_bytes =
        &data[offset..];

    let text_bytes =
        if compression_flag == 1 {
            decompress_zlib(
                text_bytes
            )?
        } else {
            text_bytes.to_vec()
        };

    // iTXt text is UTF-8.
    let text =
        String::from_utf8(
            text_bytes
        )
        .ok()?;

    Some(
        combine_keyword_text(
            &keyword,
            &text,
        )
    )
}

// ============================================================================
// ZLIB
// ============================================================================



// ============================================================================
// TEXT HELPERS
// ============================================================================

#[inline]
/// Searches the supplied input for null and returns its location or value when present.
fn find_null(
    data: &[u8],
    start: usize,
) -> Option<usize> {
    data.get(start..)?
        .iter()
        .position(
            |&byte| byte == 0
        )
        .map(
            |position| {
                start + position
            }
        )
}

#[inline]
/// Combines a parsed PNG text keyword and text payload into the representation returned by the metadata parser.
fn combine_keyword_text(
    keyword: &str,
    text: &str,
) -> String {
    let mut result =
        String::with_capacity(
            keyword.len()
                + 1
                + text.len()
        );

    result.push_str(
        keyword
    );

    result.push('\0');

    result.push_str(
        text
    );

    result
}
