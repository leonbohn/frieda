use chumsky::{prelude::*, select};

use crate::{
    AbstractLabelExpression, AcceptanceAtom, AcceptanceCondition, AcceptanceInfo,
    AcceptanceSignature, HoaBool, Id, StateConjunction, Token,
};

#[allow(unused)]
#[allow(clippy::result_large_err)]
pub fn header() -> impl Parser<Token, String, Error = Simple<Token>> + Clone {
    select! {
        Token::Header(hdr) => hdr,
    }
}

#[allow(clippy::result_large_err)]
pub fn boolean() -> impl Parser<Token, bool, Error = Simple<Token>> + Clone {
    select! {
        Token::Identifier(id) if id == *"t" => true,
        Token::Identifier(id) if id == *"f" => false,
    }
}

#[allow(clippy::result_large_err)]
pub fn integer() -> impl Parser<Token, Id, Error = Simple<Token>> + Clone {
    select! {
        Token::Int(n) => n.parse().unwrap(),
    }
}

#[allow(clippy::result_large_err)]
pub fn text() -> impl Parser<Token, String, Error = Simple<Token>> + Clone {
    select! {
        Token::Text(txt) => txt,
    }
}

#[allow(clippy::result_large_err)]
pub fn identifier() -> impl Parser<Token, String, Error = Simple<Token>> + Clone {
    select! { Token::Identifier(ident) => ident }
}

#[allow(clippy::result_large_err)]
pub fn alias_name() -> impl Parser<Token, String, Error = Simple<Token>> + Clone {
    select! { Token::Alias(aname) => aname }
}

pub fn state_conjunction() -> impl Parser<Token, StateConjunction, Error = Simple<Token>> {
    integer()
        .separated_by(just(Token::Op('&')))
        .at_least(1)
        .map(StateConjunction)
}

pub fn acceptance_signature() -> impl Parser<Token, AcceptanceSignature, Error = Simple<Token>> {
    integer()
        .repeated()
        .delimited_by(just(Token::Paren('{')), just(Token::Paren('}')))
        .map(AcceptanceSignature)
}

#[allow(clippy::result_large_err)]
pub fn acceptance_info() -> impl Parser<Token, AcceptanceInfo, Error = Simple<Token>> {
    select! {
        Token::Identifier(ident) => AcceptanceInfo::Identifier(ident),
        Token::Int(n) => AcceptanceInfo::Int(n.parse().unwrap())
    }
}

pub fn label_expression() -> impl Parser<Token, AbstractLabelExpression, Error = Simple<Token>> {
    recursive(|label_expression| {
        let value = boolean()
            .map(AbstractLabelExpression::Boolean)
            .or(integer().map(|i| AbstractLabelExpression::Integer(i as u16)));
        // .or(alias_name().map(|aname| LabelExpression::Alias(AliasName(aname))));

        let atom = value
            .or(label_expression.delimited_by(just(Token::Paren('(')), just(Token::Paren(')'))));

        let unary = just(Token::Op('!'))
            .or_not()
            .then(atom)
            .map(|(negated, expr)| {
                if negated.is_some() {
                    AbstractLabelExpression::Negated(Box::new(expr))
                } else {
                    expr
                }
            });

        let conjunction = unary
            .clone()
            .map(|x| vec![x])
            .then(just(Token::Op('&')).then(unary).repeated())
            .foldl(|mut acc, (_tok, expr)| {
                acc.push(expr);
                acc
            })
            .map(|acc| match acc.len() {
                0 => panic!("we do not expect this to happen..."),
                1 => acc.into_iter().next().unwrap(),
                _ => AbstractLabelExpression::Conjunction(acc),
            });

        conjunction
            .clone()
            .map(|conj| vec![conj])
            .then(just(Token::Op('|')).then(conjunction).repeated())
            .foldl(|mut acc, (_tok, rhs)| {
                acc.push(rhs);
                acc
            })
            .map(|acc| match acc.len() {
                0 => panic!("it seems we need to deal with the case where this is empty"),
                1 => acc.into_iter().next().unwrap(),
                _ => AbstractLabelExpression::Disjunction(acc),
            })
    })
}

pub fn acceptance_condition() -> impl Parser<Token, AcceptanceCondition, Error = Simple<Token>> {
    recursive(|acceptance_condition| {
        let atom_value = just(Token::Op('!'))
            .or_not()
            .then(integer())
            .map(|(negated, integer)| {
                if negated.is_none() {
                    AcceptanceAtom::Positive(integer)
                } else {
                    AcceptanceAtom::Negative(integer)
                }
            });

        let fin_inf_atom = just(Token::Fin)
            .to(AcceptanceCondition::Fin as fn(_) -> _)
            .or(just(Token::Inf).to(AcceptanceCondition::Inf as fn(_) -> _))
            .then(atom_value.delimited_by(just(Token::Paren('(')), just(Token::Paren(')'))))
            .map(|(f, atom)| f(atom));

        let atom =
            boolean()
                .map(HoaBool)
                .map(AcceptanceCondition::Boolean)
                .or(fin_inf_atom)
                .or(acceptance_condition
                    .delimited_by(just(Token::Paren('(')), just(Token::Paren(')'))));

        let conjunction = atom
            .clone()
            .then(
                just(Token::Op('&'))
                    .to(AcceptanceCondition::And)
                    .then(atom)
                    .repeated(),
            )
            .foldl(|lhs, (f, rhs)| f(Box::new(lhs), Box::new(rhs)));

        conjunction
            .clone()
            .then(
                just(Token::Op('|'))
                    .to(AcceptanceCondition::Or)
                    .then(conjunction)
                    .repeated(),
            )
            .foldl(|lhs, (f, rhs)| f(Box::new(lhs), Box::new(rhs)))
    })
}
