use garnish_lang_compiler::{lex, TokenType};

#[derive(Debug, Eq, PartialEq, Clone)]
struct Sink {
    annotation_text: String,
}

impl Sink {
    pub fn lone(annotation_text: String) -> Self {
        Self { annotation_text }
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
struct Collector {
    sinks: Vec<Sink>,
}

impl Collector {
    pub fn new(sinks: Vec<Sink>) -> Self {
        Self { sinks }
    }

    pub fn collect(&self, input: &str) -> Result<AnnotationInfo, String> {
        let tokens = lex(input)?;
        let mut root_annotation = AnnotationInfo::root();

        for token in tokens.iter() {
            match token.get_token_type() {
                TokenType::Annotation => root_annotation
                    .nested
                    .push(AnnotationInfo::new(token.get_text().clone())),
                _ => (),
            }
        }

        Ok(root_annotation)
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
struct AnnotationInfo {
    annotation_text: String,
    nested: Vec<AnnotationInfo>,
}

impl AnnotationInfo {
    pub fn root() -> Self {
        Self {
            annotation_text: "".to_string(),
            nested: vec![],
        }
    }

    pub fn new(annotation_text: String) -> Self {
        Self {
            annotation_text,
            nested: vec![],
        }
    }

    pub fn new_with_children(annotation_text: String, nested: Vec<AnnotationInfo>) -> Self {
        Self {
            annotation_text,
            nested,
        }
    }
}

#[cfg(test)]
mod collecting {
    use crate::collector::{AnnotationInfo, Collector, Sink};

    #[test]
    fn single_annotation() {
        let input = "@Test 5";
        let collector = Collector::new(vec![Sink::lone("@Test".to_string())]);

        let root_annotation = collector.collect(input).unwrap();

        assert_eq!(
            root_annotation,
            AnnotationInfo::new_with_children(
                "".to_string(),
                vec![AnnotationInfo::new("@Test".to_string())]
            )
        );
    }
}
