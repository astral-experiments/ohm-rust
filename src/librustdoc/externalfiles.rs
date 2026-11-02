use std::fs;
use std::path::{Path, PathBuf};

use rustc_errors::DiagCtxtHandle;
use rustc_span::FileName;
use rustc_span::edition::Edition;
use rustc_span::source_map::SourceMap;
use serde::Serialize;

use crate::html::markdown::{ErrorCodes, HeadingOffset, IdMap, Markdown, Playground};

pub(crate) struct ExternalFiles {
    pub(crate) paths: Vec<PathBuf>,
    /// Keep the original fragment text when emitting dep-info. The source map is created later.
    pub(crate) fragments: Option<Vec<(PathBuf, String)>>,
}

impl ExternalFiles {
    /// Register dependencies with the compiler using the fragment contents that rustdoc rendered.
    pub(crate) fn register(self, source_map: &SourceMap) {
        let Some(fragments) = self.fragments else { return };
        for path in self.paths {
            let _ = source_map.load_binary_file(&path);
        }
        for (path, contents) in fragments {
            let name = source_map.path_mapping().to_real_filename(source_map.working_dir(), path);
            source_map.new_source_file(FileName::Real(name), contents);
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ExternalHtml {
    /// Content that will be included inline in the `<head>` section of a
    /// rendered Markdown file or generated documentation
    pub(crate) in_header: String,
    /// Content that will be included inline between `<body>` and the content of
    /// a rendered Markdown file or generated documentation
    pub(crate) before_content: String,
    /// Content that will be included inline between the content and `</body>` of
    /// a rendered Markdown file or generated documentation
    pub(crate) after_content: String,
}

impl ExternalHtml {
    pub(crate) fn load(
        in_header: &[String],
        before_content: &[String],
        after_content: &[String],
        md_before_content: &[String],
        md_after_content: &[String],
        nightly_build: bool,
        dcx: DiagCtxtHandle<'_>,
        id_map: &mut IdMap,
        edition: Edition,
        playground: &Option<Playground>,
        fragments: &mut Option<Vec<(PathBuf, String)>>,
    ) -> Option<ExternalHtml> {
        let codes = ErrorCodes::from(nightly_build);
        let ih = load_external_files(in_header, dcx, fragments)?;
        let bc = {
            let mut bc = load_external_files(before_content, dcx, fragments)?;
            let m_bc = load_external_files(md_before_content, dcx, fragments)?;
            Markdown {
                content: &m_bc,
                links: &[],
                ids: id_map,
                error_codes: codes,
                edition,
                playground,
                heading_offset: HeadingOffset::H2,
            }
            .write_into(&mut bc)
            .unwrap();
            bc
        };
        let ac = {
            let mut ac = load_external_files(after_content, dcx, fragments)?;
            let m_ac = load_external_files(md_after_content, dcx, fragments)?;
            Markdown {
                content: &m_ac,
                links: &[],
                ids: id_map,
                error_codes: codes,
                edition,
                playground,
                heading_offset: HeadingOffset::H2,
            }
            .write_into(&mut ac)
            .unwrap();
            ac
        };
        Some(ExternalHtml { in_header: ih, before_content: bc, after_content: ac })
    }
}

pub(crate) enum LoadStringError {
    ReadFail,
    BadUtf8,
}

pub(crate) fn load_string<P: AsRef<Path>>(
    file_path: P,
    dcx: DiagCtxtHandle<'_>,
) -> Result<String, LoadStringError> {
    let file_path = file_path.as_ref();
    let contents = match fs::read(file_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            dcx.struct_err(format!(
                "error reading `{file_path}`: {e}",
                file_path = file_path.display()
            ))
            .emit();
            return Err(LoadStringError::ReadFail);
        }
    };
    match String::from_utf8(contents) {
        Ok(s) => Ok(s),
        Err(_) => {
            dcx.err(format!("error reading `{}`: not UTF-8", file_path.display()));
            Err(LoadStringError::BadUtf8)
        }
    }
}

fn load_external_files(
    names: &[String],
    dcx: DiagCtxtHandle<'_>,
    fragments: &mut Option<Vec<(PathBuf, String)>>,
) -> Option<String> {
    let mut out = String::new();
    for name in names {
        let Ok(s) = load_string(name, dcx) else { return None };
        out.push_str(&s);
        out.push('\n');
        if let Some(fragments) = fragments {
            fragments.push((PathBuf::from(name), s));
        }
    }
    Some(out)
}
