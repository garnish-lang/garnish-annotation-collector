use garnish_lang_compiler::{lex, LexerToken, TokenType};

#[derive(Debug, Eq, PartialEq, Clone)]
enum EndCondition {
    Lone,
    UntilNewline, // separate from Until because a newline in a Whitespace token needs a specific check
    TokenCount(usize),
    UntilToken(TokenType),
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Sink {
    annotation_text: String,
    end_condition: EndCondition,
    ignore_for_end_condition_list: Vec<TokenType>,
}

impl Sink {
    pub fn new<T: ToString>(annotation_text: T) -> Self {
        Self {
            annotation_text: annotation_text.to_string(),
            end_condition: EndCondition::Lone,
            ignore_for_end_condition_list: vec![TokenType::Whitespace],
        }
    }

    pub fn token_count(mut self, count: usize) -> Self {
        self.end_condition = EndCondition::TokenCount(count);
        self
    }

    pub fn until(mut self, token_type: TokenType) -> Self {
        self.end_condition = EndCondition::UntilToken(token_type);
        self
    }

    pub fn newline(mut self) -> Self {
        self.end_condition = EndCondition::UntilNewline;
        self
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

    pub fn collect(&self, input: &str) -> Result<Vec<TokenBlock>, String> {
        let tokens = lex(input)?;

        let mut blocks = vec![];
        let mut annotations_stack: Vec<(&Sink, TokenBlock)> = vec![];
        let mut nest_level = 1; // start at 1, reserving 0 for root info in case its needed

        for token in tokens.iter() {
            match token.get_token_type() {
                TokenType::StartExpression | TokenType::StartGroup | TokenType::StartSideEffect => {
                    nest_level += 1
                }
                TokenType::EndExpression | TokenType::EndGroup | TokenType::EndSideEffect => {
                    nest_level -= 1
                }
                _ => (), // nothing additional to do
            }
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
                                EndCondition::Lone => blocks.push(
                                    TokenBlock::annotation(token.get_text().clone(), nest_level),
                                ),
                                _ => {
                                    annotations_stack.push((
                                        sink,
                                        TokenBlock::annotation(
                                            token.get_text().clone(),
                                            nest_level,
                                        ),
                                    ));
                                }
                            },
                        }
                    }
                    // Not currently collecting annotation tokens
                    // add to root
                    _ => match blocks.last_mut() {
                        Some(last) => {
                            if last.annotation_text.is_empty() {
                                last.tokens.push(token.clone())
                            } else {
                                blocks
                                    .push(TokenBlock::tokens(vec![token.clone()], nest_level))
                            }
                        }
                        None => blocks
                            .push(TokenBlock::tokens(vec![token.clone()], nest_level)),
                    },
                },
                Some((sink, info)) => {
                    if !sink
                        .ignore_for_end_condition_list
                        .contains(&token.get_token_type())
                    {
                        // info.non_ignored_token_count += 1;
                    }

                    info.tokens.push(token.clone());

                    let end = match sink.end_condition {
                        EndCondition::Lone => unreachable!(), // never added to stack
                        EndCondition::TokenCount(count) => true, // info.non_ignored_token_count >= count,
                        EndCondition::UntilToken(token_type) => {
                            token_type == token.get_token_type() // && nest_level == info.nest_level
                        }
                        EndCondition::UntilNewline => token.get_text().contains("\n"),
                    };

                    if end {
                        let (_, info) = annotations_stack.pop().unwrap(); // has to exist to get to this branch
                        match annotations_stack.last_mut() {
                            None => blocks.push(info),
                            Some((_, parent)) => parent.nested.push(info),
                        }
                    }
                }
            }
        }

        Ok(blocks)
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct TokenBlock {
    annotation_text: String,
    nested: Vec<TokenBlock>,
    tokens: Vec<LexerToken>,
}

impl TokenBlock {
    pub fn new(annotation_text: String, nest_level: usize, tokens: Vec<LexerToken>) -> Self {
        Self {
            annotation_text,
            nested: vec![],
            tokens,
        }
    }

    pub fn annotation(annotation_text: String, nest_level: usize) -> Self {
        Self {
            annotation_text,
            nested: vec![],
            tokens: vec![],
        }
    }

    pub fn tokens(tokens: Vec<LexerToken>, nest_level: usize) -> Self {
        Self::new("".to_string(), nest_level, tokens)
    }

    pub fn with_children(mut self, children: Vec<TokenBlock>) -> Self {
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

    use crate::collector::{Collector, Sink, TokenBlock};

    #[test]
    fn single_annotation() {
        let input = "@Test 5";
        let collector = Collector::new(vec![Sink::new("@Test")]);

        let blocks = collector.collect(input).unwrap();

        assert_eq!(
            blocks,
            vec![
                TokenBlock::annotation("@Test".to_string(), 1),
                TokenBlock::tokens(
                    vec![
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 5),
                        LexerToken::new("5".to_string(), TokenType::Number, 0, 6),
                    ],
                    1
                )
            ]
        );
    }

    #[test]
    fn newline() {
        let input = "@Test 5 + 5   \n   5 + 5";
        let collector = Collector::new(vec![Sink::new("@Test").newline()]);

        let blocks = collector.collect(input).unwrap();

        assert_eq!(
            blocks,
            vec![
                TokenBlock::new("@Test".to_string(), 1, vec![
                    LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 5),
                    LexerToken::new("5".to_string(), TokenType::Number, 0, 6),
                    LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 7),
                    LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 8),
                    LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 9),
                    LexerToken::new("5".to_string(), TokenType::Number, 0, 10),
                    LexerToken::new("   \n   ".to_string(), TokenType::Whitespace, 0, 11),
                ]),
                TokenBlock::tokens(
                    vec![
                        LexerToken::new("5".to_string(), TokenType::Number, 1, 3),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 1, 4),
                        LexerToken::new("+".to_string(), TokenType::PlusSign, 1, 5),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 1, 6),
                        LexerToken::new("5".to_string(), TokenType::Number, 1, 7),
                    ],
                    1
                )
            ]
        );
    }

    //
    // #[test]
    // fn newline() {
    //     let input = "@Test 5 + 5   \n   5 + 5";
    //     let collector = Collector::new(vec![Sink::new("@Test").newline()]);
    //
    //     let root_annotation = collector.collect(input).unwrap();
    //
    //     assert_eq!(
    //         root_annotation,
    //         AnnotationInfo::root().with_children(vec![AnnotationInfo::new("@Test".to_string(), 1)
    //             .significant_token_count(3)
    //             .with_tokens(vec![
    //                 LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 5),
    //                 LexerToken::new("5".to_string(), TokenType::Number, 0, 6),
    //                 LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 7),
    //                 LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 8),
    //                 LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 9),
    //                 LexerToken::new("5".to_string(), TokenType::Number, 0, 10),
    //                 LexerToken::new("   \n   ".to_string(), TokenType::Whitespace, 0, 11),
    //             ])])
    //     );
    // }
    //
    // #[test]
    // fn non_annotation_tokens_added_to_root() {
    //     let input = "@Test 5+5\n5+5\n@Test 10+10";
    //     let collector = Collector::new(vec![Sink::new("@Test").newline()]);
    //
    //     let root_annotation = collector.collect(input).unwrap();
    //
    //     assert_eq!(
    //         root_annotation.tokens,
    //         vec![
    //             LexerToken::new("5".to_string(), TokenType::Number, 1, 0),
    //             LexerToken::new("+".to_string(), TokenType::PlusSign, 1, 1),
    //             LexerToken::new("5".to_string(), TokenType::Number, 1, 2),
    //             LexerToken::new("\n".to_string(), TokenType::Whitespace, 1, 3),
    //         ]
    //     );
    // }
    //
    // #[test]
    // fn with_5_tokens() {
    //     let input = "@Test 5 + 5 + 5";
    //     let collector = Collector::new(vec![Sink::new("@Test").token_count(5).ignore(vec![])]);
    //
    //     let root_annotation = collector.collect(input).unwrap();
    //
    //     assert_eq!(
    //         root_annotation,
    //         AnnotationInfo::root().with_children(vec![AnnotationInfo::new("@Test".to_string(), 1)
    //             .significant_token_count(5)
    //             .with_tokens(vec![
    //                 LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 5),
    //                 LexerToken::new("5".to_string(), TokenType::Number, 0, 6),
    //                 LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 7),
    //                 LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 8),
    //                 LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 9),
    //             ])])
    //     );
    // }
    //
    // #[test]
    // fn with_5_tokens_ignoring_white_space() {
    //     let input = "@Test 5 + 5 + 5 + 5 + 5";
    //     let collector = Collector::new(vec![Sink::new("@Test").token_count(5)]);
    //
    //     let root_annotation = collector.collect(input).unwrap();
    //
    //     assert_eq!(
    //         root_annotation,
    //         AnnotationInfo::root().with_children(vec![AnnotationInfo::new("@Test".to_string(), 1)
    //             .significant_token_count(5)
    //             .with_tokens(vec![
    //                 LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 5),
    //                 LexerToken::new("5".to_string(), TokenType::Number, 0, 6),
    //                 LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 7),
    //                 LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 8),
    //                 LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 9),
    //                 LexerToken::new("5".to_string(), TokenType::Number, 0, 10),
    //                 LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 11),
    //                 LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 12),
    //                 LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 13),
    //                 LexerToken::new("5".to_string(), TokenType::Number, 0, 14),
    //             ])])
    //     );
    // }
    //
    // #[test]
    // fn until_token() {
    //     let input = "@Test { 5 + 5 } 5 + 5";
    //     let collector = Collector::new(vec![Sink::new("@Test").until(TokenType::EndExpression)]);
    //
    //     let root_annotation = collector.collect(input).unwrap();
    //
    //     assert_eq!(
    //         root_annotation,
    //         AnnotationInfo::root().with_children(vec![AnnotationInfo::new("@Test".to_string(), 1)
    //             .significant_token_count(5)
    //             .with_tokens(vec![
    //                 LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 5),
    //                 LexerToken::new("{".to_string(), TokenType::StartExpression, 0, 6),
    //                 LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 7),
    //                 LexerToken::new("5".to_string(), TokenType::Number, 0, 8),
    //                 LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 9),
    //                 LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 10),
    //                 LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 11),
    //                 LexerToken::new("5".to_string(), TokenType::Number, 0, 12),
    //                 LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 13),
    //                 LexerToken::new("}".to_string(), TokenType::EndExpression, 0, 14),
    //             ])])
    //     );
    // }
    //
    // #[test]
    // fn until_token_ignores_nested_matching_tokens() {
    //     let input = "@Test {5,{5+5},5}";
    //     let collector = Collector::new(vec![Sink::new("@Test").until(TokenType::EndExpression)]);
    //
    //     let root_annotation = collector.collect(input).unwrap();
    //
    //     assert_eq!(
    //         root_annotation,
    //         AnnotationInfo::root().with_children(vec![AnnotationInfo::new("@Test".to_string(), 1)
    //             .significant_token_count(11)
    //             .with_tokens(vec![
    //                 LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 5),
    //                 LexerToken::new("{".to_string(), TokenType::StartExpression, 0, 6),
    //                 LexerToken::new("5".to_string(), TokenType::Number, 0, 7),
    //                 LexerToken::new(",".to_string(), TokenType::Comma, 0, 8),
    //                 LexerToken::new("{".to_string(), TokenType::StartExpression, 0, 9),
    //                 LexerToken::new("5".to_string(), TokenType::Number, 0, 10),
    //                 LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 11),
    //                 LexerToken::new("5".to_string(), TokenType::Number, 0, 12),
    //                 LexerToken::new("}".to_string(), TokenType::EndExpression, 0, 13),
    //                 LexerToken::new(",".to_string(), TokenType::Comma, 0, 14),
    //                 LexerToken::new("5".to_string(), TokenType::Number, 0, 15),
    //                 LexerToken::new("}".to_string(), TokenType::EndExpression, 0, 16),
    //             ])])
    //     );
    // }
}
