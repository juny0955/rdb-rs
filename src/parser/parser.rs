use crate::parser::{
    ast::{
        Assignment, ColumnDefinition, CreateTableStatement, DataType, Expression, InsertStatement,
        Literal, Projection, SelectStatement, Statement, UpdateStatement,
    },
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
            TokenKind::Create => Statement::CreateTable(self.parse_create_table()?),
            TokenKind::Insert => Statement::Insert(self.parse_insert()?),
            TokenKind::Update => Statement::Update(self.parse_update()?),
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

    fn parse_insert(&mut self) -> Result<InsertStatement, ParseError> {
        self.expect(TokenKind::Insert)?;
        self.expect(TokenKind::Into)?;
        let table = self.expect_identifier()?;
        self.expect(TokenKind::Values)?;
        let literals = self.parse_literals()?;

        Ok(InsertStatement { table, literals })
    }

    fn parse_update(&mut self) -> Result<UpdateStatement, ParseError> {
        self.expect(TokenKind::Update)?;
        let table = self.expect_identifier()?;
        self.expect(TokenKind::Set)?;
        let assignments = self.parse_assignments()?;
        let mut filter = None;

        if self.current().kind == TokenKind::Where {
            self.expect(TokenKind::Where)?;
            filter = Some(self.parse_equal_expression()?);
        }

        Ok(UpdateStatement {
            table,
            assignments,
            filter,
        })
    }

    fn parse_assignments(&mut self) -> Result<Vec<Assignment>, ParseError> {
        let mut assignments = Vec::new();
        assignments.push(self.parse_assignment()?);

        while self.current().kind == TokenKind::Comma {
            self.expect(TokenKind::Comma)?;
            assignments.push(self.parse_assignment()?);
        }

        Ok(assignments)
    }

    fn parse_assignment(&mut self) -> Result<Assignment, ParseError> {
        let column = self.expect_identifier()?;
        self.expect(TokenKind::Eq)?;
        let value = self.expect_literal()?;
        Ok(Assignment { column, value })
    }

    fn parse_literals(&mut self) -> Result<Vec<Literal>, ParseError> {
        self.expect(TokenKind::LeftParen)?;
        let mut literals = Vec::new();
        literals.push(self.expect_literal()?);

        while self.current().kind == TokenKind::Comma {
            self.expect(TokenKind::Comma)?;
            literals.push(self.expect_literal()?);
        }
        self.expect(TokenKind::RightParen)?;

        Ok(literals)
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

    fn parse_create_table(&mut self) -> Result<CreateTableStatement, ParseError> {
        self.expect(TokenKind::Create)?;
        self.expect(TokenKind::Table)?;
        let table = self.expect_identifier()?;
        let columns = self.parse_column_definitions()?;

        Ok(CreateTableStatement { table, columns })
    }

    fn parse_column_definitions(&mut self) -> Result<Vec<ColumnDefinition>, ParseError> {
        self.expect(TokenKind::LeftParen)?;
        let mut definitions = Vec::new();
        definitions.push(self.parse_column_definition()?);

        while self.current().kind == TokenKind::Comma {
            self.expect(TokenKind::Comma)?;
            definitions.push(self.parse_column_definition()?);
        }
        self.expect(TokenKind::RightParen)?;

        Ok(definitions)
    }

    fn parse_column_definition(&mut self) -> Result<ColumnDefinition, ParseError> {
        let name = self.expect_identifier()?;
        let data_type = self.parse_data_type()?;

        Ok(ColumnDefinition { name, data_type })
    }

    fn parse_data_type(&mut self) -> Result<DataType, ParseError> {
        let current_token = self.current();
        let data_type = match &current_token.kind {
            TokenKind::Int => DataType::Int,
            TokenKind::BigInt => DataType::BigInt,
            TokenKind::Boolean => DataType::Boolean,
            TokenKind::Varchar => DataType::Varchar,
            TokenKind::Null => DataType::Null,
            _ => return Err(ParseError::UnexpectedToken(current_token.offset)),
        };

        self.advance();
        Ok(data_type)
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
        let literal = match &current_token.kind {
            TokenKind::Integer(integer) => {
                let integer = *integer;
                Literal::Integer(integer)
            }
            TokenKind::StringLiteral(str) => {
                let str = str.clone();
                Literal::String(str)
            }
            TokenKind::Null => Literal::Null,
            _ => return Err(ParseError::UnexpectedToken(current_token.offset)),
        };

        self.advance();
        Ok(literal)
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

    #[test]
    fn create_table_문을_ast로_파싱한다() {
        let mut lexer = Lexer::new("CREATE TABLE users (id BIGINT, name VARCHAR);");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);

        let statement = parser.parse().unwrap();

        assert_eq!(
            statement,
            Statement::CreateTable(CreateTableStatement {
                table: "users".to_owned(),
                columns: vec![
                    ColumnDefinition {
                        name: "id".to_owned(),
                        data_type: DataType::BigInt,
                    },
                    ColumnDefinition {
                        name: "name".to_owned(),
                        data_type: DataType::Varchar,
                    },
                ],
            })
        );
    }

    #[test]
    fn insert_문을_ast로_파싱한다() {
        let mut lexer = Lexer::new("INSERT INTO users VALUES (1, 'Kim', NULL);");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);

        let statement = parser.parse().unwrap();

        assert_eq!(
            statement,
            Statement::Insert(InsertStatement {
                table: "users".to_owned(),
                literals: vec![
                    Literal::Integer(1),
                    Literal::String("Kim".to_owned()),
                    Literal::Null,
                ],
            })
        );
    }

    #[test]
    fn update_문을_ast로_파싱한다() {
        let mut lexer = Lexer::new("UPDATE users SET name = 'Lee' WHERE id = 1;");
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);

        let statement = parser.parse().unwrap();

        assert_eq!(
            statement,
            Statement::Update(UpdateStatement {
                table: "users".to_owned(),
                assignments: vec![Assignment {
                    column: "name".to_owned(),
                    value: Literal::String("Lee".to_owned()),
                }],
                filter: Some(Expression::Equal {
                    left: Box::new(Expression::Identifier("id".to_owned())),
                    right: Box::new(Expression::Literal(Literal::Integer(1))),
                }),
            })
        );
    }
}
