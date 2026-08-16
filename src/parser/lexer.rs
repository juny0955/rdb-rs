use crate::parser::token::{Token, TokenKind};

#[derive(Debug)]
pub enum LexError {
    UnexpectedCharacter(usize, char),
    InvalidInteger(usize),
    UnterminatedString(usize),
}

pub(crate) struct Lexer<'a> {
    input: &'a str,
    offset: usize,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(input: &'a str) -> Self {
        Self { input, offset: 0 }
    }

    pub(crate) fn tokenize(&mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            let is_eof = token.kind == TokenKind::Eof;

            tokens.push(token);

            if is_eof {
                break;
            }
        }

        Ok(tokens)
    }

    pub(crate) fn next_token(&mut self) -> Result<Token, LexError> {
        self.skip_whitespace();
        let start = self.offset;

        match self.current_char() {
            Some(ch) if ch.is_ascii_alphabetic() || ch == '_' => {
                self.advance_char();
                while self
                    .current_char()
                    .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                {
                    self.advance_char();
                }

                let word = &self.input[start..self.offset];
                let token_kind = TokenKind::from(word);
                Ok(Token::new(token_kind, start))
            }
            Some(ch) if ch.is_ascii_digit() => {
                self.advance_char();
                while self.current_char().is_some_and(|ch| ch.is_ascii_digit()) {
                    self.advance_char();
                }

                let number = self.input[start..self.offset]
                    .parse::<i64>()
                    .map_err(|_| LexError::InvalidInteger(start))?;
                Ok(Token::new(TokenKind::Integer(number), start))
            }
            Some('\'') => {
                self.advance_char();
                let current_start = self.offset;
                while let Some(ch) = self.current_char() {
                    if ch == '\'' {
                        let word = &self.input[current_start..self.offset];
                        self.advance_char();

                        return Ok(Token::new(
                            TokenKind::StringLiteral(word.to_string()),
                            start,
                        ));
                    }

                    self.advance_char();
                }

                Err(LexError::UnterminatedString(start))
            }
            Some(ch) => {
                let kind = match ch {
                    '(' => TokenKind::LeftParen,
                    ')' => TokenKind::RightParen,
                    ',' => TokenKind::Comma,
                    ';' => TokenKind::Semicolon,
                    '*' => TokenKind::Asterisk,
                    '=' => TokenKind::Eq,
                    _ => return Err(LexError::UnexpectedCharacter(start, ch)),
                };

                self.advance_char();
                Ok(Token::new(kind, start))
            }
            None => Ok(Token::new(TokenKind::Eof, start)),
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.current_char()
            && ch.is_whitespace()
        {
            self.advance_char();
        }
    }

    fn current_char(&self) -> Option<char> {
        self.input[self.offset..].chars().next()
    }

    fn advance_char(&mut self) {
        if let Some(ch) = self.current_char() {
            self.offset += ch.len_utf8();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_where_정수_토큰화() {
        let mut lexer = Lexer::new("SELECT name FROM users WHERE id = 10;");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::new(TokenKind::Select, 0),
                Token::new(TokenKind::Identifier("name".to_owned()), 7),
                Token::new(TokenKind::From, 12),
                Token::new(TokenKind::Identifier("users".to_owned()), 17),
                Token::new(TokenKind::Where, 23),
                Token::new(TokenKind::Identifier("id".to_owned()), 29),
                Token::new(TokenKind::Eq, 32),
                Token::new(TokenKind::Integer(10), 34),
                Token::new(TokenKind::Semicolon, 36),
                Token::new(TokenKind::Eof, 37),
            ]
        );
    }

    #[test]
    fn insert_리터럴_토큰화() {
        let mut lexer = Lexer::new("INSERT INTO users VALUES (1, 'kim', NULL);");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::new(TokenKind::Insert, 0),
                Token::new(TokenKind::Into, 7),
                Token::new(TokenKind::Identifier("users".to_owned()), 12),
                Token::new(TokenKind::Values, 18),
                Token::new(TokenKind::LeftParen, 25),
                Token::new(TokenKind::Integer(1), 26),
                Token::new(TokenKind::Comma, 27),
                Token::new(TokenKind::StringLiteral("kim".to_owned()), 29),
                Token::new(TokenKind::Comma, 34),
                Token::new(TokenKind::Null, 36),
                Token::new(TokenKind::RightParen, 40),
                Token::new(TokenKind::Semicolon, 41),
                Token::new(TokenKind::Eof, 42),
            ]
        );
    }

    #[test]
    fn 닫히지_않은_문자열은_오류() {
        let mut lexer = Lexer::new("'hello");
        let error = lexer.tokenize().unwrap_err();
        assert!(matches!(error, LexError::UnterminatedString(0)));
    }
}
