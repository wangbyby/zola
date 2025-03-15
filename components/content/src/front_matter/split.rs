use std::fmt::Write;
use std::path::Path;

use errors::{bail, Context, Result};
use libs::once_cell::sync::Lazy;
use libs::regex::Regex;
use libs::{serde_json, serde_yaml, toml};
use serde::Serialize;

use crate::front_matter::page::PageFrontMatter;
use crate::front_matter::section::SectionFrontMatter;

static TOML_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^[[:space:]]*\+\+\+(\r?\n(?s).*?(?-s))\+\+\+[[:space:]]*(?:$|(?:\r?\n((?s).*(?-s))$))",
    )
    .unwrap()
});

static YAML_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[[:space:]]*---(\r?\n(?s).*?(?-s))---[[:space:]]*(?:$|(?:\r?\n((?s).*(?-s))$))")
        .unwrap()
});

#[derive(Debug)]
pub enum RawFrontMatter<'a> {
    Toml(&'a str),
    Yaml(&'a str),
    Markdown { title: &'a str },
}

impl RawFrontMatter<'_> {
    pub(crate) fn deserialize<T>(&self) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let f: T = match self {
            RawFrontMatter::Toml(s) => toml::from_str(s)?,
            RawFrontMatter::Yaml(s) => match serde_yaml::from_str(s) {
                Ok(d) => d,
                Err(e) => bail!("YAML deserialize error: {:?}", e),
            },
            RawFrontMatter::Markdown { title } => {
                let mut json = String::with_capacity(32);
                json.write_str("{ \"title\": ").unwrap();
                json.write_str(&format!("{:?}", title)).unwrap();
                json.write_str("}").unwrap();
                dbg!(&json);
                let tmp = serde_json::from_str(&json);
                tmp?
            }
        };
        Ok(f)
    }
}

/// Split a file between the front matter and its content
/// Will return an error if the front matter wasn't found
fn split_content<'c>(
    file_path: &'c Path,
    content: &'c str,
) -> Result<(RawFrontMatter<'c>, &'c str)> {
    let (re, is_toml) = if TOML_RE.is_match(content) {
        (&TOML_RE as &Regex, true)
    } else if YAML_RE.is_match(content) {
        (&YAML_RE as &Regex, false)
    } else {
        // do not return error but, use the file name as title
        if let Some(title) = file_path.file_stem().map(|n| n.to_str()).flatten() {
            dbg!(title);
            return Ok((RawFrontMatter::Markdown { title: title }, content));
        }
        bail!(
            "Couldn't find front matter in `{}`. Did you forget to add `+++` or `---`?",
            file_path.to_string_lossy()
        );
    };

    // 2. extract the front matter and the content
    let caps = re.captures(content).unwrap();
    // caps[0] is the full match
    // caps[1] => front matter
    // caps[2] => content
    let front_matter = caps.get(1).unwrap().as_str();
    let content = caps.get(2).map_or("", |m| m.as_str());

    if is_toml {
        Ok((RawFrontMatter::Toml(front_matter), content))
    } else {
        Ok((RawFrontMatter::Yaml(front_matter), content))
    }
}

/// Split a file between the front matter and its content.
/// Returns a parsed `SectionFrontMatter` and the rest of the content
pub fn split_section_content<'c>(
    file_path: &'c Path,
    content: &'c str,
) -> Result<(SectionFrontMatter, &'c str)> {
    let (front_matter, content) = split_content(file_path, content)?;
    dbg!(&front_matter);
    let meta = SectionFrontMatter::parse(&front_matter).with_context(|| {
        format!("Error when parsing front matter of section `{}`", file_path.to_string_lossy())
    })?;

    Ok((meta, content))
}

/// Split a file between the front matter and its content
/// Returns a parsed `PageFrontMatter` and the rest of the content
pub fn split_page_content<'c>(
    file_path: &'c Path,
    content: &'c str,
) -> Result<(PageFrontMatter, &'c str)> {
    let (front_matter, content) = split_content(file_path, content)?;
    dbg!(&front_matter);
    let meta = PageFrontMatter::parse(&front_matter).with_context(|| {
        format!("Error when parsing front matter of section `{}`", file_path.to_string_lossy())
    })?;
    Ok((meta, content))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use test_case::test_case;

    use super::{split_page_content, split_section_content};

    #[test_case(r#"
+++
title = "Title"
description = "hey there"
date = 2002-10-12
+++
Hello
"#; "toml")]
    #[test_case(r#"
---
title: Title
description: hey there
date: 2002-10-12
---
Hello
"#; "yaml")]
    fn can_split_page_content_valid(content: &str) {
        let (front_matter, content) = split_page_content(Path::new(""), content).unwrap();
        assert_eq!(content, "Hello\n");
        assert_eq!(front_matter.title.unwrap(), "Title");
    }

    #[test_case(r#"
+++
paginate_by = 10
+++
Hello
"#; "toml")]
    #[test_case(r#"
---
paginate_by: 10
---
Hello
"#; "yaml")]
    fn can_split_section_content_valid(content: &str) {
        let (front_matter, content) = split_section_content(Path::new(""), content).unwrap();
        assert_eq!(content, "Hello\n");
        assert!(front_matter.is_paginated());
    }

    #[test_case(r#"
+++
title = "Title"
description = "hey there"
date = 2002-10-12
+++
"#; "toml")]
    #[test_case(r#"
---
title: Title
description: hey there
date: 2002-10-12
---
"#; "yaml")]
    #[test_case(r#"
+++
title = "Title"
description = "hey there"
date = 2002-10-12
+++"#; "toml no newline")]
    #[test_case(r#"
---
title: Title
description: hey there
date: 2002-10-12
---"#; "yaml no newline")]
    fn can_split_content_with_only_frontmatter_valid(content: &str) {
        let (front_matter, content) = split_page_content(Path::new(""), content).unwrap();
        assert_eq!(content, "");
        assert_eq!(front_matter.title.unwrap(), "Title");
    }

    #[test_case(r#"
+++
title = "Title"
description = "hey there"
date = 2002-10-02T15:00:00Z
+++
+++"#, "+++"; "toml with pluses in content")]
    #[test_case(r#"
+++
title = "Title"
description = "hey there"
date = 2002-10-02T15:00:00Z
+++
---"#, "---"; "toml with minuses in content")]
    #[test_case(r#"
---
title: Title
description: hey there
date: 2002-10-02T15:00:00Z
---
+++"#, "+++"; "yaml with pluses in content")]
    #[test_case(r#"
---
title: Title
description: hey there
date: 2002-10-02T15:00:00Z
---
---"#, "---"; "yaml with minuses in content")]
    fn can_split_content_lazily(content: &str, expected: &str) {
        let (front_matter, content) = split_page_content(Path::new(""), content).unwrap();
        assert_eq!(content, expected);
        assert_eq!(front_matter.title.unwrap(), "Title");
    }

    #[test_case(r#"
+++
title = "Title"
description = "hey there"
date = 2002-10-12"#; "toml")]
    #[test_case(r#"
+++
title = "Title"
description = "hey there"
date = 2002-10-12
---"#; "toml unmatched")]
    #[test_case(r#"
+++
title = "Title"
description = "hey there"
date = 2002-10-12
++++"#; "toml too many pluses")]
    #[test_case(r#"
---
title: Title
description: hey there
date: 2002-10-12"#; "yaml")]
    #[test_case(r#"
---
title: Title
description: hey there
date: 2002-10-12
+++"#; "yaml unmatched")]
    #[test_case(r#"
---
title: Title
description: hey there
date: 2002-10-12
----"#; "yaml too many dashes")]
    fn errors_if_cannot_locate_frontmatter(content: &str) {
        let res = split_page_content(Path::new(""), content);
        assert!(res.is_err());
    }
}
