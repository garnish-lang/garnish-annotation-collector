use garnish_lang_compiler::lex::{lex, LexerToken, TokenType};

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum PartBehavior {
    UntilNewline,
    TokenCount(usize),
    StartEnd { start: TokenType, end: TokenType },
    UntilToken(TokenType),
    UntilAnnotation(String),
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct PartParser {
    behavior: PartBehavior,
    ignore_tokens: Vec<TokenType>,
    trim_tokens: Vec<TokenType>,
}

impl PartParser {
    pub fn new(behavior: PartBehavior) -> Self {
        PartParser {
            behavior,
            ignore_tokens: vec![],
            trim_tokens: vec![],
        }
    }

    pub fn ignore_token(mut self, token: TokenType) -> Self {
        self.ignore_tokens.push(token);
        self
    }

    pub fn ignore_tokens(mut self, mut tokens: Vec<TokenType>) -> Self {
        self.ignore_tokens.append(&mut tokens);
        self
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
enum EndCondition {
    Lone,
    UntilNewline, // separate from Until because a newline in a Whitespace token needs a specific check
    Count(usize),
    UntilToken(TokenType),
    UntilAnnotation(String),
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct Sink {
    annotation_text: String,
    end_condition: EndCondition,
    ignore_for_end_condition_list: Vec<TokenType>,
    part_parsers: Vec<PartParser>,
}

impl Sink {
    pub fn new<T: ToString>(annotation_text: T) -> Self {
        Self {
            annotation_text: annotation_text.to_string(),
            end_condition: EndCondition::Lone,
            ignore_for_end_condition_list: vec![TokenType::Whitespace],
            part_parsers: vec![],
        }
    }

    pub fn count(mut self, count: usize) -> Self {
        self.end_condition = EndCondition::Count(count);
        self
    }

    pub fn until_token(mut self, token_type: TokenType) -> Self {
        self.end_condition = EndCondition::UntilToken(token_type);
        self
    }

    pub fn until_annotation<T: ToString>(mut self, annotation: T) -> Self {
        self.end_condition = EndCondition::UntilAnnotation(annotation.to_string());
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

    pub fn part(mut self, part_parser: PartParser) -> Self {
        self.part_parsers.push(part_parser);
        self
    }
}

struct CollectionData<'a> {
    sink: &'a Sink,
    block: TokenBlock,
    nest_level: usize,
    count: usize,
    ended: bool,
    current_part: usize,
    current_part_tokens: Vec<LexerToken>,
}

impl<'a> CollectionData<'a> {
    fn new(sink: &'a Sink, block: TokenBlock, nest_level: usize) -> Self {
        Self {
            sink,
            block,
            nest_level,
            count: 0,
            ended: false,
            current_part: 0,
            current_part_tokens: vec![],
        }
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

    pub fn collect_tokens(&self, tokens: &Vec<LexerToken>) -> Result<Vec<TokenBlock>, String> {
        let mut blocks = vec![];
        let mut annotations_stack: Vec<CollectionData> = vec![];
        let mut current_nest_level = 1; // start at 1, reserving 0 for root info in case its needed

        for token in tokens.iter() {
            match token.get_token_type() {
                TokenType::StartExpression | TokenType::StartGroup | TokenType::StartSideEffect => {
                    current_nest_level += 1
                }
                TokenType::EndExpression | TokenType::EndGroup | TokenType::EndSideEffect => {
                    current_nest_level -= 1
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
                                EndCondition::Lone => blocks
                                    .push(TokenBlock::with_annotation(token.get_text().clone())),
                                _ => {
                                    annotations_stack.push(CollectionData::new(
                                        sink,
                                        TokenBlock::with_annotation(token.get_text().clone()),
                                        current_nest_level,
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
                                blocks.push(TokenBlock::with_tokens(vec![token.clone()]))
                            }
                        }
                        None => blocks.push(TokenBlock::with_tokens(vec![token.clone()])),
                    },
                },
                Some(CollectionData {
                    sink,
                    block,
                    nest_level,
                    count,
                    ended,
                    current_part,
                    current_part_tokens,
                }) => {
                    match sink.part_parsers.get(*current_part) {
                        None => {}
                        Some(parser) => {
                            let part_ended = match parser.behavior {
                                PartBehavior::UntilNewline => token.get_text().contains("\n"),
                                _ => unimplemented!(),
                            };

                            // Don't add nested annotations to tokens if we have a sink for it
                            let nested_sink = match token.get_token_type() {
                                TokenType::Annotation => match self
                                    .sinks
                                    .iter()
                                    .find(|item| &item.annotation_text == token.get_text())
                                {
                                    // No sink for annotation, add to tokens
                                    None => {
                                        current_part_tokens.push(token.clone());
                                        None
                                    }
                                    Some(sink) => match sink.end_condition {
                                        EndCondition::Lone => {
                                            blocks.push(TokenBlock::with_annotation(
                                                token.get_text().clone(),
                                            ));
                                            None
                                        }
                                        _ => Some(sink),
                                    },
                                },
                                _ => {
                                    current_part_tokens.push(token.clone());
                                    None
                                }
                            };

                            if part_ended {
                                block.parts.push(current_part_tokens.clone());
                                *current_part_tokens = vec![];
                                *current_part = *current_part + 1;
                            }

                            *ended = *current_part >= sink.part_parsers.len();

                            match nested_sink {
                                None => (),
                                Some(sink) => {
                                    annotations_stack.push(CollectionData::new(
                                        sink,
                                        TokenBlock::with_annotation(token.get_text().clone()),
                                        current_nest_level,
                                    ));
                                }
                            }

                            // Possible to have multiple ended blocks in stack
                            // loop until all have been popped
                            while annotations_stack
                                .last()
                                .and_then(|b| Some(b.ended))
                                .unwrap_or(false)
                            {
                                let data = annotations_stack.pop().unwrap(); // has to exist to get to this branch
                                match annotations_stack.last_mut() {
                                    None => blocks.push(data.block),
                                    Some(parent) => parent.block.nested.push(data.block),
                                }
                            }
                        }
                    }

                    // if !sink
                    //     .ignore_for_end_condition_list
                    //     .contains(&token.get_token_type())
                    // {
                    //     *count += 1;
                    // }
                    //
                    // *ended = match &sink.end_condition {
                    //     EndCondition::Lone => unreachable!(), // never added to stack
                    //     EndCondition::Count(desired_count) => count == desired_count, // info.non_ignored_token_count >= count,
                    //     EndCondition::UntilToken(token_type) => {
                    //         token_type == &token.get_token_type()
                    //             && nest_level == &current_nest_level
                    //     }
                    //     EndCondition::UntilNewline => token.get_text().contains("\n"),
                    //     EndCondition::UntilAnnotation(annotation) => {
                    //         token.get_token_type() == TokenType::Annotation
                    //             && token.get_text() == annotation
                    //     }
                    // };
                    //
                    // // Don't add nested annotations to tokens if we have a sink for it
                    // match token.get_token_type() {
                    //     TokenType::Annotation => match self
                    //         .sinks
                    //         .iter()
                    //         .find(|item| &item.annotation_text == token.get_text())
                    //     {
                    //         // No sink for annotation, add to tokens
                    //         None => block.tokens.push(token.clone()),
                    //         Some(sink) => match sink.end_condition {
                    //             EndCondition::Lone => blocks
                    //                 .push(TokenBlock::with_annotation(token.get_text().clone())),
                    //             _ => {
                    //                 annotations_stack.push(CollectionData::new(
                    //                     sink,
                    //                     TokenBlock::with_annotation(token.get_text().clone()),
                    //                     current_nest_level,
                    //                 ));
                    //             }
                    //         },
                    //     },
                    //     _ => block.tokens.push(token.clone()),
                    // }
                    //
                    // // Possible to have multiple ended blocks in stack
                    // // loop until all have been popped
                    // while annotations_stack
                    //     .last()
                    //     .and_then(|b| Some(b.ended))
                    //     .unwrap_or(false)
                    // {
                    //     let data = annotations_stack.pop().unwrap(); // has to exist to get to this branch
                    //     match annotations_stack.last_mut() {
                    //         None => blocks.push(data.block),
                    //         Some(parent) => parent.block.nested.push(data.block),
                    //     }
                    // }
                }
            }
        }

        // End all blocks with end of input
        while let Some(data) = annotations_stack.pop() {
            match annotations_stack.last_mut() {
                None => blocks.push(data.block),
                Some(parent) => parent.block.nested.push(data.block),
            }
        }

        Ok(blocks)
    }

    pub fn collect_tokens_from_input(&self, input: &str) -> Result<Vec<TokenBlock>, String> {
        let tokens = lex(input)?;
        self.collect_tokens(&tokens)
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct TokenBlock {
    annotation_text: String,
    nested: Vec<TokenBlock>,
    tokens: Vec<LexerToken>,
    parts: Vec<Vec<LexerToken>>,
}

impl TokenBlock {
    pub fn new(annotation_text: String, tokens: Vec<LexerToken>) -> Self {
        Self {
            annotation_text,
            nested: vec![],
            tokens,
            parts: vec![],
        }
    }

    pub fn new_with_parts(
        annotation_text: String,
        tokens: Vec<LexerToken>,
        parts: Vec<Vec<LexerToken>>,
    ) -> Self {
        Self {
            annotation_text,
            nested: vec![],
            tokens,
            parts,
        }
    }

    pub fn with_annotation(annotation_text: String) -> Self {
        Self {
            annotation_text,
            nested: vec![],
            tokens: vec![],
            parts: vec![],
        }
    }

    pub fn with_tokens(tokens: Vec<LexerToken>) -> Self {
        Self::new("".to_string(), tokens)
    }

    pub fn and_children(mut self, children: Vec<TokenBlock>) -> Self {
        self.nested = children;
        self
    }

    pub fn and_tokens(mut self, tokens: Vec<LexerToken>) -> Self {
        self.tokens = tokens;
        self
    }

    pub fn annotation_text(&self) -> &String {
        &self.annotation_text
    }

    pub fn blocks(&self) -> &Vec<TokenBlock> {
        &self.nested
    }

    pub fn tokens(&self) -> &Vec<LexerToken> {
        &self.tokens
    }

    pub fn tokens_owned(self) -> Vec<LexerToken> {
        self.tokens
    }
}

#[cfg(test)]
mod collecting {
    use garnish_lang_compiler::lex::{LexerToken, TokenType};

    use crate::collector::{Collector, Sink, TokenBlock};
    use crate::{PartBehavior, PartParser};

    #[test]
    fn single_annotation() {
        let input = "@Test 5";
        let collector = Collector::new(vec![Sink::new("@Test")]);

        let blocks = collector.collect_tokens_from_input(input).unwrap();

        assert_eq!(
            blocks,
            vec![
                TokenBlock::with_annotation("@Test".to_string()),
                TokenBlock::with_tokens(vec![
                    LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 5),
                    LexerToken::new("5".to_string(), TokenType::Number, 0, 6),
                ])
            ]
        );
    }

    #[test]
    fn newline() {
        let input = "@Test 5 + 5   \n   5 + 5";
        let collector = Collector::new(vec![Sink::new("@Test")
            .newline()
            .part(PartParser::new(PartBehavior::UntilNewline))]);

        let blocks = collector.collect_tokens_from_input(input).unwrap();

        assert_eq!(
            blocks,
            vec![
                TokenBlock::new_with_parts(
                    "@Test".to_string(),
                    vec![],
                    vec![vec![
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 5),
                        LexerToken::new("5".to_string(), TokenType::Number, 0, 6),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 7),
                        LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 8),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 9),
                        LexerToken::new("5".to_string(), TokenType::Number, 0, 10),
                        LexerToken::new("   \n   ".to_string(), TokenType::Whitespace, 0, 11),
                    ]]
                ),
                TokenBlock::new_with_parts(
                    "".to_string(),
                    vec![
                        LexerToken::new("5".to_string(), TokenType::Number, 1, 3),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 1, 4),
                        LexerToken::new("+".to_string(), TokenType::PlusSign, 1, 5),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 1, 6),
                        LexerToken::new("5".to_string(), TokenType::Number, 1, 7),
                    ],
                    vec![]
                )
            ]
        );
    }

    #[test]
    fn with_5_tokens_ignoring_white_space() {
        let input = "@Test 5 + 5 + 5 + 5 + 5";
        let collector = Collector::new(vec![Sink::new("@Test").count(5)]);

        let blocks = collector.collect_tokens_from_input(input).unwrap();

        assert_eq!(
            blocks,
            vec![
                TokenBlock::new(
                    "@Test".to_string(),
                    vec![
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 5),
                        LexerToken::new("5".to_string(), TokenType::Number, 0, 6),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 7),
                        LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 8),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 9),
                        LexerToken::new("5".to_string(), TokenType::Number, 0, 10),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 11),
                        LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 12),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 13),
                        LexerToken::new("5".to_string(), TokenType::Number, 0, 14),
                    ]
                ),
                TokenBlock::new(
                    "".to_string(),
                    vec![
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 15),
                        LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 16),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 17),
                        LexerToken::new("5".to_string(), TokenType::Number, 0, 18),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 19),
                        LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 20),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 21),
                        LexerToken::new("5".to_string(), TokenType::Number, 0, 22),
                    ]
                )
            ]
        );
    }

    #[test]
    fn until_token() {
        let input = "@Test { 5 + 5 } 5 + 5";
        let collector = Collector::new(vec![
            Sink::new("@Test").until_token(TokenType::EndExpression)
        ]);

        let blocks = collector.collect_tokens_from_input(input).unwrap();

        assert_eq!(
            blocks,
            vec![
                TokenBlock::new(
                    "@Test".to_string(),
                    vec![
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 5),
                        LexerToken::new("{".to_string(), TokenType::StartExpression, 0, 6),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 7),
                        LexerToken::new("5".to_string(), TokenType::Number, 0, 8),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 9),
                        LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 10),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 11),
                        LexerToken::new("5".to_string(), TokenType::Number, 0, 12),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 13),
                        LexerToken::new("}".to_string(), TokenType::EndExpression, 0, 14),
                    ]
                ),
                TokenBlock::new(
                    "".to_string(),
                    vec![
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 15),
                        LexerToken::new("5".to_string(), TokenType::Number, 0, 16),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 17),
                        LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 18),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 19),
                        LexerToken::new("5".to_string(), TokenType::Number, 0, 20),
                    ]
                )
            ]
        );
    }

    #[test]
    fn unfinished_block_ended_with_end_of_input() {
        let input = "@Test { 5 + 5 }";
        let collector = Collector::new(vec![
            Sink::new("@Test").until_token(TokenType::Subexpression)
        ]);

        let blocks = collector.collect_tokens_from_input(input).unwrap();

        assert_eq!(
            blocks,
            vec![TokenBlock::new(
                "@Test".to_string(),
                vec![
                    LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 5),
                    LexerToken::new("{".to_string(), TokenType::StartExpression, 0, 6),
                    LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 7),
                    LexerToken::new("5".to_string(), TokenType::Number, 0, 8),
                    LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 9),
                    LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 10),
                    LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 11),
                    LexerToken::new("5".to_string(), TokenType::Number, 0, 12),
                    LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 13),
                    LexerToken::new("}".to_string(), TokenType::EndExpression, 0, 14),
                ]
            )]
        );
    }

    #[test]
    fn until_token_ignores_nested_matching_tokens() {
        let input = "@Test {5,{5+5},5}";
        let collector = Collector::new(vec![
            Sink::new("@Test").until_token(TokenType::EndExpression)
        ]);

        let blocks = collector.collect_tokens_from_input(input).unwrap();

        assert_eq!(
            blocks,
            vec![TokenBlock::new(
                "@Test".to_string(),
                vec![
                    LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 5),
                    LexerToken::new("{".to_string(), TokenType::StartExpression, 0, 6),
                    LexerToken::new("5".to_string(), TokenType::Number, 0, 7),
                    LexerToken::new(",".to_string(), TokenType::Comma, 0, 8),
                    LexerToken::new("{".to_string(), TokenType::StartExpression, 0, 9),
                    LexerToken::new("5".to_string(), TokenType::Number, 0, 10),
                    LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 11),
                    LexerToken::new("5".to_string(), TokenType::Number, 0, 12),
                    LexerToken::new("}".to_string(), TokenType::EndExpression, 0, 13),
                    LexerToken::new(",".to_string(), TokenType::Comma, 0, 14),
                    LexerToken::new("5".to_string(), TokenType::Number, 0, 15),
                    LexerToken::new("}".to_string(), TokenType::EndExpression, 0, 16),
                ]
            )]
        );
    }

    #[test]
    fn until_annotation() {
        let input = "@Test 5 + 5 @End 5 + 5";
        let collector = Collector::new(vec![Sink::new("@Test").until_annotation("@End")]);

        let blocks = collector.collect_tokens_from_input(input).unwrap();

        assert_eq!(
            blocks,
            vec![
                TokenBlock::new(
                    "@Test".to_string(),
                    vec![
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 5),
                        LexerToken::new("5".to_string(), TokenType::Number, 0, 6),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 7),
                        LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 8),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 9),
                        LexerToken::new("5".to_string(), TokenType::Number, 0, 10),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 11),
                        LexerToken::new("@End".to_string(), TokenType::Annotation, 0, 12),
                    ]
                ),
                TokenBlock::new(
                    "".to_string(),
                    vec![
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 16),
                        LexerToken::new("5".to_string(), TokenType::Number, 0, 17),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 18),
                        LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 19),
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 20),
                        LexerToken::new("5".to_string(), TokenType::Number, 0, 21),
                    ]
                )
            ]
        );
    }

    #[test]
    fn with_children() {
        let input = "@Test 5+5\n@Case 10+10\n@Case 20+20\n@End";
        let collector = Collector::new(vec![
            Sink::new("@Test").until_annotation("@End"),
            Sink::new("@Case").newline(),
        ]);

        let blocks = collector.collect_tokens_from_input(input).unwrap();

        assert_eq!(
            blocks,
            vec![TokenBlock::new(
                "@Test".to_string(),
                vec![
                    LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 5),
                    LexerToken::new("5".to_string(), TokenType::Number, 0, 6),
                    LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 7),
                    LexerToken::new("5".to_string(), TokenType::Number, 0, 8),
                    LexerToken::new("\n".to_string(), TokenType::Whitespace, 0, 9),
                    LexerToken::new("@End".to_string(), TokenType::Annotation, 3, 0),
                ]
            )
            .and_children(vec![
                TokenBlock::new(
                    "@Case".to_string(),
                    vec![
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 1, 5),
                        LexerToken::new("10".to_string(), TokenType::Number, 1, 6),
                        LexerToken::new("+".to_string(), TokenType::PlusSign, 1, 8),
                        LexerToken::new("10".to_string(), TokenType::Number, 1, 9),
                        LexerToken::new("\n".to_string(), TokenType::Whitespace, 1, 11),
                    ]
                ),
                TokenBlock::new(
                    "@Case".to_string(),
                    vec![
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 2, 5),
                        LexerToken::new("20".to_string(), TokenType::Number, 2, 6),
                        LexerToken::new("+".to_string(), TokenType::PlusSign, 2, 8),
                        LexerToken::new("20".to_string(), TokenType::Number, 2, 9),
                        LexerToken::new("\n".to_string(), TokenType::Whitespace, 2, 11),
                    ]
                )
            ]),]
        );
    }

    #[test]
    fn child_annotations_count_as_one_for_count_condition() {
        let input = "@Test 5+5\n@Case 10+10\n@Case 20+20\n5+5";
        let collector = Collector::new(vec![
            Sink::new("@Test").count(5),
            Sink::new("@Case").newline(),
        ]);

        let blocks = collector.collect_tokens_from_input(input).unwrap();

        assert_eq!(
            blocks,
            vec![
                TokenBlock::new(
                    "@Test".to_string(),
                    vec![
                        LexerToken::new(" ".to_string(), TokenType::Whitespace, 0, 5),
                        LexerToken::new("5".to_string(), TokenType::Number, 0, 6),
                        LexerToken::new("+".to_string(), TokenType::PlusSign, 0, 7),
                        LexerToken::new("5".to_string(), TokenType::Number, 0, 8),
                        LexerToken::new("\n".to_string(), TokenType::Whitespace, 0, 9),
                    ]
                )
                .and_children(vec![
                    TokenBlock::new(
                        "@Case".to_string(),
                        vec![
                            LexerToken::new(" ".to_string(), TokenType::Whitespace, 1, 5),
                            LexerToken::new("10".to_string(), TokenType::Number, 1, 6),
                            LexerToken::new("+".to_string(), TokenType::PlusSign, 1, 8),
                            LexerToken::new("10".to_string(), TokenType::Number, 1, 9),
                            LexerToken::new("\n".to_string(), TokenType::Whitespace, 1, 11),
                        ]
                    ),
                    TokenBlock::new(
                        "@Case".to_string(),
                        vec![
                            LexerToken::new(" ".to_string(), TokenType::Whitespace, 2, 5),
                            LexerToken::new("20".to_string(), TokenType::Number, 2, 6),
                            LexerToken::new("+".to_string(), TokenType::PlusSign, 2, 8),
                            LexerToken::new("20".to_string(), TokenType::Number, 2, 9),
                            LexerToken::new("\n".to_string(), TokenType::Whitespace, 2, 11),
                        ]
                    )
                ]),
                TokenBlock::with_tokens(vec![
                    LexerToken::new("5".to_string(), TokenType::Number, 3, 0),
                    LexerToken::new("+".to_string(), TokenType::PlusSign, 3, 1),
                    LexerToken::new("5".to_string(), TokenType::Number, 3, 2),
                ])
            ]
        );
    }
}
