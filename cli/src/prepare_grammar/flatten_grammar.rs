use super::ExtractedSyntaxGrammar;
use crate::error::Result;
use crate::grammars::{Production, ProductionStep, SyntaxGrammar, SyntaxVariable, Variable};
use crate::rules::{Alias, Associativity, Rule};

struct RuleFlattener {
    production: Production,
    precedence_stack: Vec<i32>,
    associativity_stack: Vec<Associativity>,
    alias_stack: Vec<Alias>,
}

impl RuleFlattener {
    fn new() -> Self {
        Self {
            production: Production {
                steps: Vec::new(),
                dynamic_precedence: 0,
            },
            precedence_stack: Vec::new(),
            associativity_stack: Vec::new(),
            alias_stack: Vec::new(),
        }
    }

    fn flatten(mut self, rule: Rule) -> Production {
        self.apply(rule, true);
        self.production
    }

    fn apply(&mut self, rule: Rule, at_end: bool) {
        match rule {
            Rule::Seq(members) => {
                let last_index = members.len() - 1;
                for (i, member) in members.into_iter().enumerate() {
                    self.apply(member, i == last_index && at_end);
                }
            }
            Rule::Metadata { rule, params } => {
                let mut has_precedence = false;
                if let Some(precedence) = params.precedence {
                    has_precedence = true;
                    self.precedence_stack.push(precedence);
                }

                let mut has_associativity = false;
                if let Some(associativity) = params.associativity {
                    has_associativity = true;
                    self.associativity_stack.push(associativity);
                }

                let mut has_alias = false;
                if let Some(alias) = params.alias {
                    has_alias = true;
                    self.alias_stack.push(alias);
                }

                if params.dynamic_precedence.abs() > self.production.dynamic_precedence.abs() {
                    self.production.dynamic_precedence = params.dynamic_precedence;
                }

                self.apply(*rule, at_end);

                if has_precedence {
                    self.precedence_stack.pop();
                    if !at_end {
                        self.production.steps.last_mut().unwrap().precedence =
                            self.precedence_stack.last().cloned().unwrap_or(0);
                    }
                }

                if has_associativity {
                    self.associativity_stack.pop();
                    if !at_end {
                        self.production.steps.last_mut().unwrap().associativity =
                            self.associativity_stack.last().cloned();
                    }
                }

                if has_alias {
                    self.alias_stack.pop();
                }
            }
            Rule::Symbol(symbol) => {
                self.production.steps.push(ProductionStep {
                    symbol,
                    precedence: self.precedence_stack.last().cloned().unwrap_or(0),
                    associativity: self.associativity_stack.last().cloned(),
                    alias: self.alias_stack.last().cloned(),
                });
            }
            _ => (),
        }
    }
}

fn extract_choices(rule: Rule) -> Vec<Rule> {
    match rule {
        Rule::Seq(elements) => {
            let mut result = vec![Rule::Blank];
            for element in elements {
                let extraction = extract_choices(element);
                let mut next_result = Vec::new();
                for entry in result {
                    for extraction_entry in extraction.iter() {
                        next_result.push(Rule::Seq(vec![entry.clone(), extraction_entry.clone()]));
                    }
                }
                result = next_result;
            }
            result
        }
        Rule::Choice(elements) => {
            let mut result = Vec::new();
            for element in elements {
                for rule in extract_choices(element) {
                    result.push(rule);
                }
            }
            result
        }
        Rule::Metadata { rule, params } => extract_choices(*rule)
            .into_iter()
            .map(|rule| Rule::Metadata {
                rule: Box::new(rule),
                params: params.clone(),
            })
            .collect(),
        _ => vec![rule],
    }
}

fn flatten_variable(variable: Variable) -> Result<SyntaxVariable> {
    let mut productions = Vec::new();
    for rule in extract_choices(variable.rule) {
        let production = RuleFlattener::new().flatten(rule);
        if !productions.contains(&production) {
            productions.push(production);
        }
    }
    Ok(SyntaxVariable {
        name: variable.name,
        kind: variable.kind,
        productions,
    })
}

pub(super) fn flatten_grammar(grammar: ExtractedSyntaxGrammar) -> Result<SyntaxGrammar> {
    let mut variables = Vec::new();
    for variable in grammar.variables {
        variables.push(flatten_variable(variable)?);
    }
    Ok(SyntaxGrammar {
        extra_tokens: grammar.extra_tokens,
        expected_conflicts: grammar.expected_conflicts,
        variables_to_inline: grammar.variables_to_inline,
        external_tokens: grammar.external_tokens,
        word_token: grammar.word_token,
        variables,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammars::VariableType;
    use crate::rules::Symbol;

    #[test]
    fn test_flatten_grammar() {
        let result = flatten_variable(Variable {
            name: "test".to_string(),
            kind: VariableType::Named,
            rule: Rule::seq(vec![
                Rule::non_terminal(1),
                Rule::prec_left(
                    101,
                    Rule::seq(vec![
                        Rule::non_terminal(2),
                        Rule::choice(vec![
                            Rule::prec_right(
                                102,
                                Rule::seq(vec![Rule::non_terminal(3), Rule::non_terminal(4)]),
                            ),
                            Rule::non_terminal(5),
                        ]),
                        Rule::non_terminal(6),
                    ]),
                ),
                Rule::non_terminal(7),
            ]),
        })
        .unwrap();

        assert_eq!(
            result.productions,
            vec![
                Production {
                    dynamic_precedence: 0,
                    steps: vec![
                        ProductionStep::new(Symbol::non_terminal(1)),
                        ProductionStep::new(Symbol::non_terminal(2))
                            .with_prec(101, Some(Associativity::Left)),
                        ProductionStep::new(Symbol::non_terminal(3))
                            .with_prec(102, Some(Associativity::Right)),
                        ProductionStep::new(Symbol::non_terminal(4))
                            .with_prec(101, Some(Associativity::Left)),
                        ProductionStep::new(Symbol::non_terminal(6)),
                        ProductionStep::new(Symbol::non_terminal(7)),
                    ]
                },
                Production {
                    dynamic_precedence: 0,
                    steps: vec![
                        ProductionStep::new(Symbol::non_terminal(1)),
                        ProductionStep::new(Symbol::non_terminal(2))
                            .with_prec(101, Some(Associativity::Left)),
                        ProductionStep::new(Symbol::non_terminal(5))
                            .with_prec(101, Some(Associativity::Left)),
                        ProductionStep::new(Symbol::non_terminal(6)),
                        ProductionStep::new(Symbol::non_terminal(7)),
                    ]
                },
            ]
        );
    }

    #[test]
    fn test_flatten_grammar_with_maximum_dynamic_precedence() {
        let result = flatten_variable(Variable {
          name: "test".to_string(),
          kind: VariableType::Named,
          rule: Rule::seq(vec![
            Rule::non_terminal(1),
            Rule::prec_dynamic(101, Rule::seq(vec![
              Rule::non_terminal(2),
              Rule::choice(vec![
                Rule::prec_dynamic(102, Rule::seq(vec![
                  Rule::non_terminal(3),
                  Rule::non_terminal(4)
                ])),
                Rule::non_terminal(5),
              ]),
              Rule::non_terminal(6),
            ])),
            Rule::non_terminal(7),
          ])
        }).unwrap();

        assert_eq!(result.productions, vec![
            Production {
                dynamic_precedence: 102,
                steps: vec![
                    ProductionStep::new(Symbol::non_terminal(1)),
                    ProductionStep::new(Symbol::non_terminal(2)),
                    ProductionStep::new(Symbol::non_terminal(3)),
                    ProductionStep::new(Symbol::non_terminal(4)),
                    ProductionStep::new(Symbol::non_terminal(6)),
                    ProductionStep::new(Symbol::non_terminal(7)),
                ],
            },
            Production {
                dynamic_precedence: 101,
                steps: vec![
                    ProductionStep::new(Symbol::non_terminal(1)),
                    ProductionStep::new(Symbol::non_terminal(2)),
                    ProductionStep::new(Symbol::non_terminal(5)),
                    ProductionStep::new(Symbol::non_terminal(6)),
                    ProductionStep::new(Symbol::non_terminal(7)),
                ],
            },
        ]);
    }

    #[test]
    fn test_flatten_grammar_with_final_precedence() {
        let result = flatten_variable(Variable {
            name: "test".to_string(),
            kind: VariableType::Named,
            rule: Rule::prec_left(101, Rule::seq(vec![
                Rule::non_terminal(1),
                Rule::non_terminal(2),
            ])),
        }).unwrap();

        assert_eq!(result.productions, vec![
            Production {
                dynamic_precedence: 0,
                steps: vec![
                    ProductionStep::new(Symbol::non_terminal(1)).with_prec(101, Some(Associativity::Left)),
                    ProductionStep::new(Symbol::non_terminal(2)).with_prec(101, Some(Associativity::Left)),
                ]
            }
        ]);

        let result = flatten_variable(Variable {
            name: "test".to_string(),
            kind: VariableType::Named,
            rule: Rule::prec_left(101, Rule::seq(vec![
                Rule::non_terminal(1),
            ])),
        }).unwrap();

        assert_eq!(result.productions, vec![
            Production {
                dynamic_precedence: 0,
                steps: vec![
                    ProductionStep::new(Symbol::non_terminal(1)).with_prec(101, Some(Associativity::Left)),
                ]
            }
        ]);
    }
}
