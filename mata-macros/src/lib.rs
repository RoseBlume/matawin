use mataimage::Image;

use proc_macro::{
    Ident,
    Literal,
    Punct,
    Spacing,
    TokenStream,
    TokenTree,
};

use std::{
    env,
    fmt::Write,
    fs,
    path::{
        Path,
        PathBuf,
    },
};

/// Decode an image while the program is being compiled.
///
/// The resulting program contains only the already-decoded pixel
/// data. No filesystem access or image decoding occurs when the
/// game starts.
///
/// # Const
///
/// ```ignore
/// preload_image!(
///     pub const PLAYER = "assets/player.png"
/// );
/// ```
///
/// # Static
///
/// For large game assets, `static` is recommended:
///
/// ```ignore
/// preload_image!(
///     pub static BACKGROUND = "assets/background.png"
/// );
/// ```
///
/// The generated item has a type of:
///
/// ```ignore
/// mataimage::PreloadedImage<N>
/// ```
///
/// where `N` is automatically calculated from the decoded image.
#[proc_macro]
pub fn preload_image(
    input: TokenStream,
) -> TokenStream {
    match expand_preload_image(input) {
        Ok(tokens) => tokens,

        Err(error) => {
            compile_error(&error)
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ItemKind {
    Const,
    Static,
}

struct Arguments {
    public: bool,
    kind: ItemKind,
    name: String,
    path: String,
}

fn expand_preload_image(
    input: TokenStream,
) -> Result<TokenStream, String> {
    let args =
        parse_arguments(input)?;

    let manifest_dir =
        env::var("CARGO_MANIFEST_DIR")
            .map_err(|_| {
                String::from(
                    "CARGO_MANIFEST_DIR is not available"
                )
            })?;

    let path =
        resolve_path(
            &manifest_dir,
            &args.path,
        );

    let bytes =
        fs::read(&path)
            .map_err(|error| {
                format!(
                    "failed to read image '{}': {}",
                    path.display(),
                    error,
                )
            })?;

    /*
     * This is the important part:
     *
     * MataImage performs the actual image decoding while the
     * procedural macro is executing during compilation.
     */
    let image =
        Image::from_bytes(&bytes)
            .map_err(|error| {
                format!(
                    "failed to decode '{}': {:?}",
                    path.display(),
                    error,
                )
            })?;

    let frame =
        image.frames
            .first()
            .ok_or_else(|| {
                format!(
                    "image '{}' contains no frames",
                    path.display(),
                )
            })?;

    let width =
        image.width;

    let height =
        image.height;

    let expected =
        width as usize *
        height as usize;

    if frame.pixels.len() != expected {
        return Err(
            format!(
                concat!(
                    "decoded image '{}' contains {} pixels, ",
                    "but {}x{} requires {} pixels"
                ),
                path.display(),
                frame.pixels.len(),
                width,
                height,
                expected,
            )
        );
    }

    generate(
        &args,
        &path,
        width,
        height,
        &frame.pixels,
    )
}

fn resolve_path(
    manifest_dir: &str,
    path: &str,
) -> PathBuf {
    let path =
        Path::new(path);

    if path.is_absolute() {
        path.to_owned()
    } else {
        Path::new(manifest_dir)
            .join(path)
    }
}

fn generate(
    arguments: &Arguments,
    source_path: &Path,
    width: u32,
    height: u32,
    pixels: &[u32],
) -> Result<TokenStream, String> {
    let mut output =
        String::with_capacity(
            pixels.len()
                .saturating_mul(14)
                .saturating_add(512)
        );

    /*
     * Cause Cargo/rustc to track changes to the original image.
     */
    let dependency_path =
        format!(
            "{:?}",
            source_path.to_string_lossy()
        );

    writeln!(
        output,
        "const _: &[u8] = include_bytes!({dependency_path});"
    )
    .map_err(|_| {
        String::from(
            "failed to generate asset dependency"
        )
    })?;

    if arguments.public {
        output.push_str("pub ");
    }

    match arguments.kind {
        ItemKind::Const => {
            output.push_str("const ");
        }

        ItemKind::Static => {
            output.push_str("static ");
        }
    }

    write!(
        output,
        concat!(
            "{}: ",
            "::mata_engine::image::PreloadedImage<{}> = ",
            "::mata_engine::image::PreloadedImage {{ ",
            "width: {}u32, ",
            "height: {}u32, ",
            "pixels: ["
        ),
        arguments.name,
        pixels.len(),
        width,
        height,
    )
    .map_err(|_| {
        String::from(
            "failed to generate image declaration"
        )
    })?;

    for &pixel in pixels {
        write!(
            output,
            "0x{pixel:08X}u32,"
        )
        .map_err(|_| {
            String::from(
                "failed to generate image pixels"
            )
        })?;
    }

    output.push_str(
        "] };"
    );

    output
        .parse()
        .map_err(|_| {
            String::from(
                "failed to parse generated image"
            )
        })
}

fn parse_arguments(
    input: TokenStream,
) -> Result<Arguments, String> {
    let mut tokens =
        input.into_iter()
            .peekable();

    let public =
        match tokens.peek() {
            Some(TokenTree::Ident(ident))
                if ident.to_string() == "pub" =>
            {
                tokens.next();
                true
            }

            _ => false,
        };

    let kind_token =
        tokens.next()
            .ok_or_else(|| {
                String::from(
                    "expected `const` or `static`"
                )
            })?;

    let kind =
        match kind_token {
            TokenTree::Ident(ident) => {
                match ident.to_string().as_str() {
                    "const" => {
                        ItemKind::Const
                    }

                    "static" => {
                        ItemKind::Static
                    }

                    _ => {
                        return Err(
                            String::from(
                                "expected `const` or `static`"
                            )
                        );
                    }
                }
            }

            _ => {
                return Err(
                    String::from(
                        "expected `const` or `static`"
                    )
                );
            }
        };

    let name =
        match tokens.next() {
            Some(TokenTree::Ident(ident)) => {
                ident.to_string()
            }

            _ => {
                return Err(
                    String::from(
                        "expected image constant name"
                    )
                );
            }
        };

    match tokens.next() {
        Some(TokenTree::Punct(punct))
            if punct.as_char() == '=' => {}

        _ => {
            return Err(
                String::from(
                    "expected `=` after image name"
                )
            );
        }
    }

    let literal =
        match tokens.next() {
            Some(TokenTree::Literal(literal)) => {
                literal.to_string()
            }

            _ => {
                return Err(
                    String::from(
                        "expected image path string"
                    )
                );
            }
        };

    if tokens.next().is_some() {
        return Err(
            String::from(
                "unexpected tokens after image path"
            )
        );
    }

    let path =
        parse_string_literal(
            &literal,
        )?;

    Ok(
        Arguments {
            public,
            kind,
            name,
            path,
        }
    )
}

fn parse_string_literal(
    literal: &str,
) -> Result<String, String> {
    if literal.len() < 2
        || !literal.starts_with('"')
        || !literal.ends_with('"')
    {
        return Err(
            String::from(
                concat!(
                    "image path must be a normal string literal; ",
                    "for example \"assets/player.png\""
                )
            )
        );
    }

    let inner =
        &literal[
            1..
            literal.len() - 1
        ];

    /*
     * Asset paths should normally use `/` even on Windows:
     *
     *     "assets/player.png"
     *
     * We handle the common escaped characters anyway.
     */
    unescape_string(inner)
}

fn unescape_string(
    input: &str,
) -> Result<String, String> {
    let mut result =
        String::with_capacity(
            input.len()
        );

    let mut chars =
        input.chars();

    while let Some(character) =
        chars.next()
    {
        if character != '\\' {
            result.push(character);
            continue;
        }

        let escaped =
            chars.next()
                .ok_or_else(|| {
                    String::from(
                        "unterminated escape in image path"
                    )
                })?;

        match escaped {
            '\\' => result.push('\\'),
            '"' => result.push('"'),
            'n' => result.push('\n'),
            'r' => result.push('\r'),
            't' => result.push('\t'),

            other => {
                return Err(
                    format!(
                        "unsupported escape sequence `\\{other}` in image path"
                    )
                );
            }
        }
    }

    Ok(result)
}

fn compile_error(
    error: &str,
) -> TokenStream {
    let escaped =
        format!("{error:?}");

    format!(
        "compile_error!({escaped});"
    )
    .parse()
    .unwrap()
}