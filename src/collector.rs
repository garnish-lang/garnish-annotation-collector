use garnish_lang_compiler::{lex, LexerToken, TokenType};

#[derive(Debug, Eq, PartialEq, Clone)]
enum EndCondition {
    Lone,
    TokenCount(usize),
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Sink {
    annotation_text: String,
    end_condition: EndCondition,
    ignore_for_end_condition_list: Vec<TokenType>,
}

impl Sink {
    pub fn lone(annotation_text: String) -> Self {
        Self {
            annotation_text,
            end_condition: EndCondition::Lone,
            ignore_for_end_condition_list: vec![],
        }
    }

    pub fn tokens(annotation_text: String, count: usize) -> Self {
        Self {
            annotation_text,
            end_condition: EndCondition::TokenCount(count),
            ignore_for_end_condition_list: vec![],
        }
    }

    pub fn ignore(mut self, tokens: Vec<TokenType>) -> Self {
        self.ignore_for_end_condition_list = tokens;
        self
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Collector {
    sinks: Vec<Sink>,
}

impl Collector {
    pub fn new(sinks: Vec<Sink>) -> Self {
        Self { sinks }
    }

    pub fn collect(&self, input: &str) -> Result<AnnotationInfo, String> {
        let tokens = lex(input)?;
        let mut root_annotation = AnnotationInfo::root();
        let mut annotations_stack: Vec<(&Sink, AnnotationInfo)> = vec![];

        for token in tokens.iter() {
            match annotations_stack.last_mut() {
                None => match token.get_token_type() {
                    TokenType::Annotation => {
                        match self
                            .sinks
                            .iter()
                            .find(|item| &item.annotation_text == token.get_text())
                        {
                            None => (), // No sink for annotation, leave be
                            Some(sink) => match sink.end_condition {
                                EndCondition::Lone => root_annotation
                                    .nested
                                    .push(AnnotationInfo::new(token.get_text().clone())),
                                _ => {
                                    annotations_stack.push((
                                        sink,
                                        AnnotationInfo::new(token.get_text().clone()),
                                    ));
                                }
                            },
                        }
                    }
                    _ => (),
                },
                Some((sink, info)) => {
                    match sink.end_condition {
                        EndCondition::Lone => unreachable!(), // never added to stack
                        EndCondition::TokenCount(count) => {
                            if !sink.ignore_for_end_condition_list.contains(&token.get_token_type()) {
                                info.tokens.push(token.clone());
                                if info.tokens.len() >= count {
                                    let (_, info) = annotations_stack.pop().unwrap(); // has exist to get to this branch
                                    match annotations_stack.last_mut() {
                                        None => root_annotation.nested.push(info),
                                        Some((_, parent)) => parent.nested.push(info),
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(root_annotation)
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct AnnotationInfo {
    annotation_text: String,
    nested: Vec<AnnotationInfo>,
    tokens: Vec<LexerToken>,
}

impl AnnotationInfo {
    pub fn root() -> Self {
        Self {
            annotation_text: "".to_string(),
            nested: vec![],
            tokens: vec![],
        }
    }

    pub fn new(annotation_text: String) -> Self {
        Self {
            annotation_text,
            nested: vec![],
            tokens: vec![],
        }
    }

    pub fn with_children(mut self, children: Vec<AnnotationInfo>) -> Self {
        self.nested = children;
        self
    }

    pub fn with_tokens(mut self, tokens: Vec<LexerToken>) -> Self {
        self.tokens = tokens;
        self
    }
}

#[cfg(test)]
mod collecting {
    use garnish_lang_compiler::{LexerToken, TokenType};

    use crate::collector::{AnnotationInfo, Collector, Sink};

    #[test]
    fn single_annotation() {
        let input = "@Test 5";
        let collector = Collector::new(vec![Sink::lone("@Test".to_string())]);

        let root_annotation = collector.collect(input).unwrap();

        assert_eq!(
            root_annotation,
            AnnotationInfo::new("".to_string())
                .with_children(vec![AnnotationInfo::new("@Test".to_string())])
        );
    }

    #[test]
    fn with_5_tokens() {
        let input = "@Test 5 + 5 + 5";
        let collector = Collector::new(vec![Sink::tokens("@Test".to_string(), 5)]);

        let root_annotation = collector.collect(input).unwrap();

        assert_eq!(
            root_annotation,
            AnnotationInfo::new("".to_string()).with_children(vec![AnnotationInfo::new(
                "@Test".to_string()
            )
            .with_tokens(vec![
                LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 5),
                LexerToken::new("5".to_string(), TokenType::Number, 0, 6),
                LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 7),
                LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 8),
                LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 9),
            ])])
        );
    }

    #[test]
    fn with_5_tokens_ignoring_white_space() {
        let input = "@Test 5 + 5 + 5 + 5 + 5";
        let collector = Collector::new(vec![
            Sink::tokens("@Test".to_string(), 5).ignore(vec![TokenType::Whitespace])
        ]);

        let root_annotation = collector.collect(input).unwrap();

        assert_eq!(
            root_annotation,
            AnnotationInfo::new("".to_string()).with_children(vec![AnnotationInfo::new(
                "@Test".to_string()
            )
            .with_tokens(vec![
                LexerToken::new("5".to_string(), TokenType::Number, 0, 6),
                LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 8),
                LexerToken::new("5".to_string(), TokenType::Number, 0, 10),
                LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 12),
                LexerToken::new("5".to_string(), TokenType::Number, 0, 14),
            ])])
        );
    }
}
