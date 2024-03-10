use super::{Node, NodeContent, ParseError, ParserContext, FilePosition};
use super::custom::CustomTag;


struct PartialNode {
    children: Vec<Node>,
    content: Vec<super::NodeContent>,
}


struct MathParseInfo {
    got_nothing: bool,
}


#[derive(PartialEq, Eq)]
enum MathStopType {
    MathEnd,
    OneThing,
    Brace,
    Parenthesis,
    SquareBracket,
}


enum MathToken<'a> {
    Alias(&'a Alias),
    Operator(&'a CustomTag),
    Other((char, FilePosition)),
    EscapedCharacter((char, FilePosition)),
    Child(usize),
    OpeningBrace,
    ClosingBrace,
    OpeningParenthesis,
    ClosingParenthesis,
    OpeningSquareBracket,
    ClosingSquareBracket,
}


enum PotentialChild {
    Some(Node),
    None((FilePosition, usize)) // Contains (start position, source length)
}


struct Alias {
    alias: &'static str,
    tag_name: &'static str,
    is_infix: bool,
}

macro_rules! alias {
    ($alias: literal, $tag_name: literal, $infix: expr) => {
        Alias { alias: $alias, tag_name: $tag_name, is_infix: $infix }
    };
}


/// HashMap of all aliases, each character maps to th corresponding default custom tag name 
static ALIASES: [Alias; 30] = [
    alias!("=", "equal", false),
    alias!(",", "comma", false),
    alias!("/", "frac", true),
    alias!("v/", "sqrt", false),
    alias!("+", "plus", false),
    alias!("-", "minus", false),
    alias!("€", "belongsto", false),
    alias!("^", "exponent", true),
    alias!("_", "subscript", true),
    alias!("^^", "overset", true),
    alias!("__", "underset", true),
    alias!("|", "normalfont", false),
    alias!("->", "rightarrow", false),
    alias!("=>", "rightdoublearrow", false),
    alias!("-->", "longrightarrow", false),
    alias!("==>", "longrightdoublearrow", false),
    alias!("<-", "leftarrow", false),
    alias!("<=", "leftdoublearrow", false),
    alias!("<--", "longleftarrow", false),
    alias!("<==", "longleftdoublearrow", false),
    alias!("<-->", "longleftrightarrow", false),
    alias!("<=>", "leftrightdoublearrow", false),
    alias!("<==>", "longleftrightdoublearrow", false),
    alias!("~", "equiv", false),
    alias!("<", "less", false),
    alias!(">", "greater", false),
    alias!(">=", "geq", false),
    alias!("=<", "leq", false),
    alias!("<<", "mless", false),
    alias!(">>", "mgreater", false),
];


/// Create math! Called after tags are parsed. Will replace the provided Node's contents by math.
/// 
/// # Arguments
/// * `node`: A node. It's children are fully math-parsed, but not it's inner text
/// 
/// # Returns
/// The node that contains all the math.
/// 
pub fn parse_math(node: &mut Node, context: &ParserContext) -> Result<(), ParseError> {
    let mut pos = 0;

    // Remove children from node to take ownership
    let raw_children = std::mem::replace(&mut node.children, vec![]);
    let mut children = raw_children.into_iter().map(|el| { PotentialChild::Some(el) }).collect();

    let (res, _) = parse_math_part(node, &mut children, &mut pos, context, MathStopType::MathEnd)?;

    // Replace node's contents
    node.children = res.children;
    node.content = res.content;

    return Ok(()); 
}


/// Sub-function of parse_math. `pos` is the position in the node's content array
fn parse_math_part(node: &mut Node, children: &mut Vec<PotentialChild>, index: &mut usize, context: &ParserContext, how_to_stop: MathStopType) 
    -> Result<(PartialNode, MathParseInfo), ParseError> {
    let mut res: Vec<NodeContent> = Vec::with_capacity(node.content.len());
    let mut res_children: Vec<Node> = Vec::with_capacity(5);

    let mut got_one_thing = false;

    loop {
        // Check for end of math
        if *index >= node.content.len() { // TEST
            if how_to_stop != MathStopType::MathEnd {
                let position = get_file_pos_of_node_char(node, *index);
                
                return Err(ParseError {
                    message: String::from("Unexpected end of math or closing tag."),
                    position,
                    length: 1,
                });
            }  
            else {
                break;
            }
        }

        let next_token = match_next_thing_in_math(node, index, &children, context)?;

        match next_token {
            MathToken::Alias(alias) => {
                let tag = context.math_operators.get(alias.tag_name);

                match tag {
                    Some(tag) => {
                        let (file_pos, _) = get_file_pos_of_node_content(node, &children, *index);

                        let alias_len = alias.alias.chars().count();
                        *index += alias_len;

                        let mut arguments: Vec<Node> = Vec::new();
                        if alias.is_infix {
                            if tag.arguments.len() != 2 { // Invalid argument count
                                return Err(ParseError { 
                                    message: format!("Operator alias \"{}\" in infix but corresponds to the operator \"{}\" with {} arguments. This is probably because you have modified default.cowx.", alias.alias, alias.tag_name, tag.arguments.len()), 
                                    position: file_pos, length: alias_len 
                                })
                            }

                            // Nothing before infix operator
                            if res.len() == 0 {
                                return Err(ParseError { 
                                    message: format!("Expected something before \"{}\", because it's an infix operator. You should write {{left}}{}{{right}} instead of {}{{left}}{{right}}", alias.alias, alias.alias, alias.alias), 
                                    position: file_pos, length: alias_len 
                                });
                            }

                            // Gt the last element of the nodes's content
                            let left = match res.pop().unwrap() {
                                NodeContent::Character(c) |
                                NodeContent::EscapedCharacter(c) => Node { 
                                    name: String::from("div"), 
                                    attributes: vec![], 
                                    children: vec![], 
                                    content: vec![NodeContent::Character(c)], 
                                    auto_closing: false, 
                                    declared_with_question_mark: false, 
                                    start_position: file_pos.clone(),
                                    start_inner_position: file_pos, 
                                    source_length: 1 
                                },
                                NodeContent::Child(_) => {
                                    res_children.pop().unwrap()
                                },
                            };

                            if *index >= node.content.len() {
                                let (position, _) = get_file_pos_of_node_content(node, children, node.content.len() - 1);
                                return Err(ParseError { 
                                    message: format!("Expected something after \"{}\" because it's an infix operator.", alias.alias), 
                                    position, length: 1,
                                });
                            }

                            let (right, info) = parse_math_subgroup(node, children, index, context, MathStopType::OneThing)?;

                            if info.got_nothing {
                                let (position, _) = get_file_pos_of_node_content(node, children, *index - 1);
                                return Err(ParseError { 
                                    message: format!("Expected something after \"{}\" because it's an infix operator.", alias.alias), 
                                    position, length: 1,
                                });
                            }

                            arguments.push(left);
                            arguments.push(right);
                        }
                        else {
                            for i in 0..tag.arguments.len() {
                                // Reached the end: not enough arguments
                                if *index >= node.content.len() {
                                    let (position, _) = get_file_pos_of_node_content(node, children, node.content.len() - 1);
                                    return Err(ParseError { 
                                        message: format!("Expected something here. \"{}\" expects {} arguments, and you provided only {}", alias.alias, tag.arguments.len(), i), 
                                        position, length: 1,
                                    });
                                }

                                let (child, info) = parse_math_subgroup(node, children, index, context, MathStopType::OneThing)?;

                                if info.got_nothing {
                                    let (position, _) = get_file_pos_of_node_content(node, children, *index - 1);
                                    return Err(ParseError { 
                                        message: format!("Expected something here. \"{}\" expects {} arguments, and you provided only {}", alias.alias, tag.arguments.len(), i), 
                                        position, length: 1,
                                    });
                                }

                                arguments.push(child);
                            }
                        }

                        let instantiated = super::custom::instantiate_tag(tag, arguments);
                        let new_child_id = res_children.len();
                        res_children.push(instantiated);
                        res.push(NodeContent::Child(new_child_id));

                        got_one_thing = true;
                    },
                    None => {
                        let (position, _) = get_file_pos_of_node_content(node, children, *index);
                        return Err(ParseError { 
                            message: format!("Operator alias \"{}\" found, but corresponding tag \"{}\" not found. This is probably because you have modified default.cowx.", alias.alias, alias.tag_name), 
                            position, length: alias.alias.len()
                        });
                    }
                }
            },
            MathToken::Operator(op) => {
                let mut arguments = Vec::with_capacity(op.arguments.len());
                for i in 0..op.arguments.len() {
                    if *index >= node.content.len() {
                        let (position, _) = get_file_pos_of_node_content(node, children, node.content.len() - 1);
                        return Err(ParseError { 
                            message: format!("Expected something here. \"{}\" expects {} arguments, and you provided only {}", op.content.name, op.arguments.len(), i), 
                            position, length: 1,
                        });
                    }

                    let (child, info) = parse_math_subgroup(node, children, index, context, MathStopType::OneThing)?;
                    arguments.push(child);

                    if info.got_nothing {
                        let (position, _) = get_file_pos_of_node_content(node, children, *index - 1);
                        return Err(ParseError { 
                            message: format!("Expected something here. \"{}\" expects {} arguments, and you provided only {}", op.content.name, op.arguments.len(), i), 
                            position, length: 1,
                        });
                    }
                }
        
                let instantiated = super::custom::instantiate_tag(op, arguments);

                let new_child_id = res_children.len();
                res_children.push(instantiated);
                res.push(NodeContent::Child(new_child_id));
                got_one_thing = true;
            },
            MathToken::OpeningBrace => { // Sub group. Make a recursive call
                *index += 1;

                let (child, _) = parse_math_subgroup(node, children, index, context, MathStopType::Brace)?;

                let new_child_id = res_children.len();
                res_children.push(child);
                res.push(NodeContent::Child(new_child_id));
                got_one_thing = true;
            },
            MathToken::ClosingBrace => {
                *index += 1;

                if how_to_stop != MathStopType::Brace {
                    return Err(report_stop_error(node, how_to_stop, &next_token, *index));
                }

                break;
            },
            MathToken::ClosingParenthesis | MathToken::ClosingSquareBracket => {
                *index += 1;

                let op_name = match next_token {
                    MathToken::ClosingParenthesis => "closingparenthesis",
                    MathToken::ClosingSquareBracket => "closingsquarebracket",
                    _ => unreachable!(),
                };

                let operator = match context.math_operators.get(op_name) {
                    Some(op) => op,
                    None => {
                        return Err(ParseError {
                            message: format!("Operator \"{}\" not found! This may be because you modified default.cowx.", op_name),
                            position: get_file_pos_of_node_char(node, *index - 1),
                            length: 1,
                        });
                    }
                };

                // Add a parenthesis before finish
                res.push(NodeContent::Child(res_children.len()));
                res_children.push(super::custom::instantiate_tag(operator, vec![]));

                if !compare_math_token_and_math_stop(&next_token, &how_to_stop) {
                    return Err(report_stop_error(node, how_to_stop, &next_token, *index));
                }

                break;
            },
            MathToken::OpeningParenthesis | MathToken::OpeningSquareBracket => {
                *index += 1;

                let op_name = match next_token {
                    MathToken::OpeningParenthesis => "openingparenthesis",
                    MathToken::OpeningSquareBracket => "openingsquarebracket",
                    _ => unreachable!(),
                };

                let operator = match context.math_operators.get(op_name) {
                    Some(op) => op,
                    None => {
                        return Err(ParseError {
                            message: format!("Operator \"{}\" not found! This may be because you modified default.cowx.", op_name),
                            position: get_file_pos_of_node_char(node, *index - 1),
                            length: 1,
                        });
                    }
                };

                let stop_type = match next_token {
                    MathToken::OpeningParenthesis => MathStopType::Parenthesis,
                    MathToken::OpeningSquareBracket => MathStopType::SquareBracket,
                    _ => unreachable!(),
                };

                let (mut child, _) = parse_math_subgroup(node, children, index, context, stop_type)?;

                // Add a parenthesis at th beginning of the child
                child.content.insert(0, NodeContent::Child(child.children.len()));
                child.children.push(super::custom::instantiate_tag(operator, vec![]));

                let new_child_id = res_children.len();
                res_children.push(child);
                res.push(NodeContent::Child(new_child_id));
                got_one_thing = true;
            },
            MathToken::Other(('§', _)) => {
                *index += 1;

                let (letter_to_convert, letter_position) = match &node.content[*index] {
                    NodeContent::Character(l) => l.clone(),
                    NodeContent::EscapedCharacter(l) => {
                        let (position, length) = get_file_pos_of_node_content(node, children, *index);
                        crate::log::warning_position(
                            "Escaped character after \"§\". Consider removing the backslash.", 
                            &position, length
                        );
                        l.clone()
                    },
                    NodeContent::Child(_) => {
                        let (position, length) = get_file_pos_of_node_content(node, children, *index);
                        return Err(ParseError {
                            message: format!("Expected a character after \"§\", found a tag."),
                            position, length
                        });
                    },
                };

                let greek_letter = letter_to_greek(letter_to_convert);
                match greek_letter {
                    Some(l) => {
                        res.push(NodeContent::Character((l, letter_position)));
                        *index += 1;
                        got_one_thing = true;
                    },
                    None => {
                        let (position, length) = get_file_pos_of_node_content(node, children, *index);
                        return Err(ParseError {
                            message: format!("Character \"{}\" after \"§\" does not correspond to a greek letter. Only a-z, A-Z are accepted, except for q, Q, w and W", letter_to_convert),
                            position, length
                        });
                    },
                }
            },
            MathToken::Other((c, file_position)) => {
                if c.is_whitespace() { // Ignore whitespace!
                    *index += 1;
                }
                else { // A normal character
                    res.push(NodeContent::Character((c, file_position)));
                    *index += 1;
                    got_one_thing = true;
                }
            },
            MathToken::EscapedCharacter(c) => {
                res.push(NodeContent::Character(c));
                *index += 1;
                got_one_thing = true;
            },
            MathToken::Child(c) => { // A child, just push it as a normal NodeContent
                let source_infos = match &children[c] {
                    PotentialChild::Some(child) => (child.start_position.clone(), child.source_length),
                    PotentialChild::None(..) => panic!("Should be Some"),
                };

                let child = std::mem::replace(&mut children[c], PotentialChild::None(source_infos));

                match child {
                    PotentialChild::Some(child) =>{
                        res_children.push(child);
                        res.push(NodeContent::Child(res_children.len() - 1));
                        
                        *index += 1;
                    },
                    PotentialChild::None(_) => unreachable!(),
                }

                got_one_thing = true;
            },
        }

        if got_one_thing && how_to_stop == MathStopType::OneThing {
            break;
        }
    }

    return Ok((
        PartialNode {
            children: res_children,
            content: res,
        },
        MathParseInfo {
            got_nothing: !got_one_thing,
        }
    ));
}


fn match_next_thing_in_math<'a>(node: &mut Node, index: &mut usize, children: &Vec<PotentialChild>, context: &'a ParserContext) -> Result<MathToken<'a>, ParseError> {
    let alias = if context.ignore_aliases { None } else { check_for_alias(&node.content, *index) };

    match alias {
        Some(alias) => return Ok(MathToken::Alias(alias)),
        None => {
            match &node.content[*index] {
                NodeContent::Character((c, pos)) => {
                    if *c == '?' {
                        *index += 1; // TODO: berk
                        let op = expect_operator(node, &children, index, context)?;
                        return Ok(MathToken::Operator(op));
                    }
                    else if *c == '{' {
                        return Ok(MathToken::OpeningBrace);
                    }
                    else if *c == '}' {
                        return Ok(MathToken::ClosingBrace);
                    }
                    else if *c == '(' {
                        return Ok(MathToken::OpeningParenthesis);
                    }
                    else if *c == ')' {
                        return Ok(MathToken::ClosingParenthesis);
                    }
                    else if *c == '[' {
                        return Ok(MathToken::OpeningSquareBracket);
                    }
                    else if *c == ']' {
                        return Ok(MathToken::ClosingSquareBracket);
                    }
                    else {
                        return Ok(MathToken::Other((*c, pos.clone())));
                    }
                },
                NodeContent::EscapedCharacter(c) => return Ok(MathToken::EscapedCharacter(c.clone())),
                NodeContent::Child(c) => return Ok(MathToken::Child(*c)),
            }
        },
    }

    // TODO
}


/// Tries to read an operator AFTER the question mark
fn expect_operator<'a>(node: &Node, children: &Vec<PotentialChild>, pos: &mut usize, context: &'a ParserContext) -> Result<&'a CustomTag, ParseError> {
    let mut word = String::with_capacity(15);
    let start_pos = *pos - 1;

    while *pos < node.content.len() {
        let el = &node.content[*pos];

        match *el {
            NodeContent::Character((c, _)) => {
                if super::WORD_CHARS.contains(c) || c.is_alphabetic() {
                    word.push(c);
                }
                else {
                    break;
                }
            },
            NodeContent::EscapedCharacter(_) => {
                break;
            },
            NodeContent::Child(_) => {
                break;
            },
        }

        *pos += 1;
    }

    if word == "" {
        let (position, _) = get_file_pos_of_node_content(node, children, start_pos);
        return Err(ParseError { 
            message: format!("Nothing found after \"?\". Question marks are used for operators. If you wanted to add a question mark in math, put a backslash before: \"\\?\""), 
            position, 
            length: word.len() + 1,
        });
    }

    match context.math_operators.get(&word) {
        Some(op) => return Ok(op),
        None => {
            let (position, _) = get_file_pos_of_node_content(node, children, start_pos);
            return Err(ParseError { 
                message: format!("Unknown math operator name \"{}\"", word), 
                position, 
                length: word.len() + 1,
            });
        }
    }

}


/// Returns the proper error if a tag is present instead of a character
fn expect_character(node: &Node, children: &Vec<PotentialChild>, id: usize) -> Result<char, ParseError> {
    match node.content[id] {
        NodeContent::Character((c, _)) => return Ok(c),
        _ => {
            let (position, length) = get_file_pos_of_node_content(node, children, id);
            return Err(ParseError { message: String::from("Didn't expected a tag here."), position, length });
        }
    }
}


/// Returns the source position and source length of a content element in a node
///
/// # Arguments
/// * `children`: the children of the node (in case they are separated)
/// * `content_id`: the position of the desired content in the content array of the node.
fn get_file_pos_of_node_content(node: &Node, children: &Vec<PotentialChild>, content_id: usize) -> (FilePosition, usize) {
    let id = std::cmp::min(content_id, node.content.len() - 1);

    match &node.content[id] {
        NodeContent::Character((_, pos)) => {
            return (pos.clone(), 1);
        },
        NodeContent::EscapedCharacter((_, pos)) => {
            return (pos.clone(), 2);
        },
        NodeContent::Child(c) => {
            match &children[*c] {
                PotentialChild::Some(child) => {
                    return (child.start_position.clone(), child.source_length);
                },
                PotentialChild::None((start_pos, source_len)) => {
                    return (start_pos.clone(), *source_len);
                },
            }
        }
    };
}


// Same as `get_file_pos_of_node_content`, but panics if a child is found and returns only a position
fn get_file_pos_of_node_char(node: &Node, content_id: usize) -> FilePosition {
    let id = std::cmp::min(content_id, node.content.len() - 1);

    match &node.content[id] {
        NodeContent::Character((_, pos)) => {
            return pos.clone();
        },
        NodeContent::EscapedCharacter((_, pos)) => {
            return pos.clone();
        },
        NodeContent::Child(_) => panic!("This shouldn't have been called.")
    };
}


// OPTI: that's O(n²) because of chars().nth(). Also a lot of vec allocations
/// Returns the longest possible alias at specified position, returns None if no alias found 
fn check_for_alias(node_content: &Vec<NodeContent>, index: usize) -> Option<&'static Alias> {
    let mut potential_matchs: Vec<usize> = Vec::with_capacity(ALIASES.len());
    for i in 0..ALIASES.len() {
        potential_matchs.push(i);
    }

    let mut res = None;

    let mut pos = 0;
    loop {
        let mut new_vec = Vec::new();
        for i in potential_matchs.iter() {
            let opt_char = ALIASES[*i].alias.chars().nth(pos);

            match opt_char {
                Some(alias_char) => {
                    let node_char;

                    if index + pos >= node_content.len() {
                        node_char = '\0';
                    }
                    else {
                        node_char = match node_content[index + pos] {
                            NodeContent::Character((c, _)) => c,
                            NodeContent::EscapedCharacter(_) => '\0',
                            NodeContent::Child(_) => '\0',
                        };
                    }
        
                    let still_ok = alias_char == node_char;
        
                    if still_ok {
                        new_vec.push(*i);
                    }
                },
                None => { // End of the alias
                    res = Some(&ALIASES[*i]); // Set the result
                },
            }

        }

        if new_vec.len() == 0 { // No more potential matches
            break;
        }

        potential_matchs = new_vec;
        pos += 1;
    }

    return res;
}


// Helper for parse_math_part
fn parse_math_subgroup(node: &mut Node, children: &mut Vec<PotentialChild>, index: &mut usize, context: &ParserContext, how_to_stop: MathStopType) 
    -> Result<(Node, MathParseInfo), ParseError> {
    let start_pos = *index;
    let (start_position, _) = get_file_pos_of_node_content(node, children, *index);

    let (partial_child, info) = parse_math_part(node, children, index, context, how_to_stop)?;

    let res = Node {
        name: String::from("div"),
        attributes: vec![],
        children: partial_child.children,
        content: partial_child.content,
        auto_closing: false,
        declared_with_question_mark: false, 
        start_position: start_position.clone(),
        start_inner_position: start_position,
        source_length: *index - start_pos,
    };

    return Ok((res, info));
}


/// Converts a char to greek, returns None if non-alphabetical, Q or W
fn letter_to_greek(c: char) -> Option<char> {
    let ascii_code = c as u8;

    if !c.is_ascii() { return None; }

    if 'A' <= c && c <= 'Z' && c != 'Q' && c != 'W' {
        return Some("ΑΒΨΔΕΦΓΗΙΞΚΛΜΝΟΠ ΡΣΤΘΩ ΧΥΖ".chars().nth((ascii_code - ('A' as u8)) as usize).expect("Uuh?"));
    }
    else if 'a' <= c && c <= 'z' && c != 'q' && c != 'w' {
        return Some("αβψδεφγηιξκλμνοπ ρστθω χυζ".chars().nth((ascii_code - ('a' as u8)) as usize).expect("Uuh?"));
    }

    return None;
}


/// Helper for parse_math_part
/// Reports errors due to unexpected braces and similar 
fn report_stop_error(node: &Node, expected: MathStopType, found: &MathToken, index: usize) -> ParseError {
    let position = get_file_pos_of_node_char(node, index - 1);

    let found_str = match found {
        MathToken::OpeningBrace => "{",
        MathToken::ClosingBrace => "}",
        MathToken::OpeningParenthesis => "(",
        MathToken::ClosingParenthesis => ")",
        MathToken::OpeningSquareBracket => "[",
        MathToken::ClosingSquareBracket => "]",
        _ => panic!("Uuh?")
    };
    
    match expected {
        MathStopType::MathEnd => ParseError {
            message: format!("Unmatched \"{}\".", found_str),
            position: position,
            length: 1,
        },
        MathStopType::OneThing => ParseError {
            message: format!("Expected something before \"{}\".", found_str),
            position: position,
            length: 1,
        },
        MathStopType::Brace => ParseError {
            message: format!("Unmatched \"{}\". Opened with \"{{\", but closed with \"{}\".", found_str, found_str),
            position: position,
            length: 1,
        },
        MathStopType::Parenthesis => ParseError {
            message: format!("Unmatched \"{}\". Opened with \"(\", but closed with \"{}\".", found_str, found_str),
            position: position,
            length: 1,
        },
        MathStopType::SquareBracket => ParseError {
            message: format!("Unmatched \"{}\". Opened with \"[\", but closed with \"{}\".", found_str, found_str),
            position: position,
            length: 1,
        },
    }
}


// Helper for parse_math_part
fn compare_math_token_and_math_stop(token: &MathToken, stop: &MathStopType) -> bool {
    match stop {
        MathStopType::MathEnd => false,
        MathStopType::OneThing => false,
        MathStopType::Brace => match token {
            MathToken::ClosingBrace => true,
            _ => false
        },
        MathStopType::Parenthesis => match token {
            MathToken::ClosingParenthesis => true,
            _ => false
        },
        MathStopType::SquareBracket => match token {
            MathToken::ClosingSquareBracket => true,
            _ => false
        },
    }
}

