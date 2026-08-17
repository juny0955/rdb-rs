use crate::parser::{
    ast::{Expression, Literal, Projection, SelectStatement, Statement},
    token::{Token, TokenKind},
};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ParseError {
    UnexpectedToken(usize),
}

pub(crate) struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    pub(crate) fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            position: 0,
        }
    }

    pub(crate) fn parse(&mut self) -> Result<Statement, ParseError> {
        let current = self.current();
        let statement = match current.kind {
            TokenKind::Select => Statement::Select(self.parse_select()?),
            _ => return Err(ParseError::UnexpectedToken(current.offset)),
        };

        self.expect(TokenKind::Semicolon)?;
        self.expect(TokenKind::Eof)?;

        Ok(statement)
    }

    fn parse_select(&mut self) -> Result<SelectStatement, ParseError> {
        self.expect(TokenKind::Select)?;
        let projections = self.parse_projections()?;
        self.expect(TokenKind::From)?;
        let table = self.expect_identifier()?;
        let mut filter = None;

        if self.current().kind == TokenKind::Where {
            self.expect(TokenKind::Where)?;
            filter = Some(self.parse_equal_expression()?);
        }

        Ok(SelectStatement {
            projections,
            table,
            filter,
        })
    }

    fn parse_projections(&mut self) -> Result<Vec<Projection>, ParseError> {
        let mut projections = Vec::new();
        projections.push(self.parse_projection()?);

        while self.current().kind == TokenKind::Comma {
            self.expect(TokenKind::Comma)?;
            projections.push(self.parse_projection()?);
        }

        Ok(projections)
    }

    fn parse_projection(&mut self) -> Result<Projection, ParseError> {
        let current = self.current();
        match &current.kind {
            TokenKind::Asterisk => {
                self.expect(TokenKind::Asterisk)?;
                Ok(Projection::All)
            }
            TokenKind::Identifier(_) => {
                let expression = Expression::Identifier(self.expect_identifier()?);
                Ok(Projection::Expression(expression))
            }
            _ => Err(ParseError::UnexpectedToken(current.offset)),
        }
    }

    fn parse_equal_expression(&mut self) -> Result<Expression, ParseError> {
        let identifier = Expression::Identifier(self.expect_identifier()?);
        self.expect(TokenKind::Eq)?;
        let literal = Expression::Literal(self.expect_literal()?);

        Ok(Expression::Equal {
            left: Box::new(identifier),
            right: Box::new(literal),
        })
    }

    fn expect_identifier(&mut self) -> Result<String, ParseError> {
        let current_token = self.current();
        match &current_token.kind {
            TokenKind::Identifier(name) => {
                let name = name.clone();
                self.advance();
                Ok(name)
            }
            _ => Err(ParseError::UnexpectedToken(current_token.offset)),
        }
    }

    fn expect_literal(&mut self) -> Result<Literal, ParseError> {
        let current_token = self.current();
        match &current_token.kind {
            TokenKind::Integer(integer) => {
                let integer = *integer;
                self.advance();
                Ok(Literal::Integer(integer))
            }
            TokenKind::StringLiteral(str) => {
                let str = str.clone();
                self.advance();
                Ok(Literal::String(str))
            }
            TokenKind::Null => {
                self.advance();
                Ok(Literal::Null)
            }
            _ => Err(ParseError::UnexpectedToken(current_token.offset)),
        }
    }

    fn expect(&mut self, expected: TokenKind) -> Result<(), ParseError> {
        let current_token = self.current();
        if current_token.kind != expected {
            return Err(ParseError::UnexpectedToken(current_token.offset));
        }

        self.advance();
        Ok(())
    }

    fn current(&self) -> &Token {
        &self.tokens[self.position]
    }

    fn advance(&mut self) {
        if self.current().kind != TokenKind::Eof {
            self.position += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::lexer::Lexer;

    use super::*;

    #[test]
    fn select_where_문을_ast로_파싱한다() {
        let mut lexer = Lexer::new("SELECT name FROM users WHERE id = 10;");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);

        let statement = parser.parse().unwrap();

        assert_eq!(
            statement,
            Statement::Select(SelectStatement {
                projections: vec![Projection::Expression(Expression::Identifier(
                    "name".to_owned()
                ))],
                table: "users".to_owned(),
                filter: Some(Expression::Equal {
                    left: Box::new(Expression::Identifier("id".to_owned())),
                    right: Box::new(Expression::Literal(Literal::Integer(10))),
                }),
            })
        );
    }
}
