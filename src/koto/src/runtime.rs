#![macro_use]

use koto_parser::{AstNode, AstOp, Node, Position};
use std::{collections::HashMap, fmt, rc::Rc};

use crate::{
    call_stack::CallStack,
    return_stack::ReturnStack,
    value::{MultiRangeValueIterator, Value, ValueIterator},
    Id, LookupId,
};

#[derive(Debug)]
pub enum Error {
    RuntimeError {
        message: String,
        start_pos: Position,
        end_pos: Position,
    },
}

pub type RuntimeResult = Result<(), Error>;
pub type BuiltinResult = Result<Value, String>;

#[derive(Debug)]
pub struct Scope {
    values: HashMap<Rc<String>, Value>,
}

impl Scope {
    fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    fn print_keys(&self) {
        println!(
            "{:?}",
            self.values
                .keys()
                .map(|key| key.as_ref().clone())
                .collect::<Vec<_>>()
        );
    }
}

macro_rules! make_runtime_error {
    ($node:expr, $message:expr) => {
        Error::RuntimeError {
            message: $message,
            start_pos: $node.start_pos,
            end_pos: $node.end_pos,
        }
    };
}

macro_rules! runtime_error {
    ($node:expr, $error:expr) => {
        Err(make_runtime_error!($node, String::from($error)))
    };
    ($node:expr, $error:expr, $($y:expr),+) => {
        Err(make_runtime_error!($node, format!($error, $($y),+)))
    };
}

pub type BuiltinFunction<'a> = Box<dyn FnMut(&[Value]) -> BuiltinResult + 'a>;

pub enum BuiltinValue<'a> {
    Function(BuiltinFunction<'a>),
    Map(BuiltinMap<'a>),
}

impl<'a> fmt::Display for BuiltinValue<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use BuiltinValue::*;
        match self {
            Function(_) => write!(f, "Builtin Function"),
            Map(_) => write!(f, "Builtin Map"),
        }
    }
}

pub struct BuiltinMap<'a>(Vec<(String, BuiltinValue<'a>)>);

impl<'a> BuiltinMap<'a> {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn add_map(&mut self, name: &str) -> &mut BuiltinMap<'a> {
        self.insert(name, BuiltinValue::Map(BuiltinMap::new()));

        if let BuiltinValue::Map(map) = self.get_entry_mut(name).unwrap() {
            return map;
        }

        unreachable!();
    }

    pub fn add_fn(&mut self, name: &str, f: impl FnMut(&[Value]) -> BuiltinResult + 'a) {
        self.insert(name, BuiltinValue::Function(Box::new(f)));
    }

    pub fn get_mut(&mut self, lookup_id: &[Id]) -> Option<&mut BuiltinValue<'a>> {
        use BuiltinValue::*;

        match self.get_entry_mut(lookup_id.first().unwrap().as_ref()) {
            Some(value) => {
                if lookup_id.len() == 1 {
                    Some(value)
                } else {
                    match value {
                        Map(map) => map.get_mut(&lookup_id[1..]),
                        Function(_) => None,
                    }
                }
            }
            None => None,
        }
    }

    fn insert(&mut self, name: &str, value: BuiltinValue<'a>) {
        if let Some(existing) = self.get_entry_mut(name) {
            *existing = value;
        } else {
            self.0.push((name.to_string(), value));
        }
    }

    fn get_entry_mut(&mut self, name: &str) -> Option<&mut BuiltinValue<'a>> {
        for (entry_name, value) in self.0.iter_mut() {
            if entry_name == name {
                return Some(value);
            }
        }
        None
    }
}

pub struct Runtime<'a> {
    global: Scope,
    builtins: BuiltinMap<'a>,
    call_stack: CallStack,
    return_stack: ReturnStack,
}

#[cfg(feature = "trace")]
macro_rules! runtime_trace  {
    ($self:expr, $message:expr) => {
        println!("{}{}", $self.runtime_indent(), $message);
    };
    ($self:expr, $message:expr, $($vals:expr),+) => {
        println!("{}{}", $self.runtime_indent(), format!($message, $($vals),+));
    };
}

#[cfg(not(feature = "trace"))]
macro_rules! runtime_trace {
    ($self:expr, $message:expr) => {};
    ($self:expr, $message:expr, $($vals:expr),+) => {};
}

impl<'a> Runtime<'a> {
    pub fn new() -> Self {
        let mut result = Self {
            global: Scope::new(),
            builtins: BuiltinMap::new(),
            call_stack: CallStack::new(),
            return_stack: ReturnStack::new(),
        };
        crate::builtins::register(&mut result);
        result
    }

    pub fn builtins_mut(&mut self) -> &mut BuiltinMap<'a> {
        return &mut self.builtins;
    }

    /// Run a script and capture the final value
    pub fn run(&mut self, ast: &Vec<AstNode>) -> Result<Value, Error> {
        runtime_trace!(self, "run");
        self.return_stack.start_frame();

        self.evaluate_block(ast)?;

        match self.return_stack.values() {
            [] => Ok(Value::Empty),
            [single_value] => Ok(single_value.clone()),
            values @ _ => {
                let list = Value::List(Rc::new(values.to_owned()));
                Ok(list)
            }
        }
    }

    /// Evaluate a series of expressions and keep the final result on the return stack
    fn evaluate_block(&mut self, block: &Vec<AstNode>) -> RuntimeResult {
        runtime_trace!(self, "evaluate_block - {}", block.len());

        self.return_stack.start_frame();

        for (i, expression) in block.iter().enumerate() {
            if i < block.len() - 1 {
                self.evaluate_and_expand(expression)?;
                self.return_stack.pop_frame();
            } else {
                self.evaluate_and_capture(expression)?;
                self.return_stack.pop_frame_and_keep_results();
            }
        }

        Ok(())
    }

    /// Evaluate a series of expressions and add their results to the return stack
    fn evaluate_expressions(&mut self, expressions: &Vec<AstNode>) -> RuntimeResult {
        runtime_trace!(self, "evaluate_expressions - {}", expressions.len());

        self.return_stack.start_frame();

        for expression in expressions.iter() {
            self.evaluate_and_capture(expression)?;
            self.return_stack.pop_frame_and_keep_results();
        }

        Ok(())
    }

    /// Evaluate an expression and capture multiple return values in a List
    ///
    /// Single return values get left on the stack without allocation
    fn evaluate_and_capture(&mut self, expression: &AstNode) -> RuntimeResult {
        use Value::*;

        runtime_trace!(self, "evaluate_and_capture - {}", expression.node);

        self.return_stack.start_frame();

        self.evaluate_and_expand(expression)?;

        match self.return_stack.value_count() {
            0 => {
                self.return_stack.pop_frame();
                self.return_stack.push(Empty);
            }
            1 => {
                self.return_stack.pop_frame_and_keep_results();
            }
            _ => {
                // TODO check values in return stack for unexpanded for loops + ranges
                let list = self
                    .return_stack
                    .values()
                    .iter()
                    .cloned()
                    .map(|value| match value {
                        For(_) | Range { .. } => runtime_error!(
                            expression,
                            "Invalid value found in list capture: '{}'",
                            value
                        ),
                        _ => Ok(value),
                    })
                    .collect::<Result<Vec<_>, Error>>()?;
                self.return_stack.pop_frame();
                self.return_stack.push(List(Rc::new(list)));
            }
        }

        Ok(())
    }

    /// Evaluates a single expression, and expands single return values
    ///
    /// A single For loop or Range in first position will be expanded
    fn evaluate_and_expand(&mut self, expression: &AstNode) -> RuntimeResult {
        runtime_trace!(self, "evaluate_and_expand - {}", expression.node);

        self.return_stack.start_frame();

        self.evaluate(expression)?;

        if self.return_stack.values().len() == 1 {
            let value = self.return_stack.value().clone();
            self.return_stack.pop_frame();

            use Value::*;
            match value {
                For(_) => {
                    self.run_for_loop(&value, expression)?;
                    let loop_value_count = self.return_stack.value_count();
                    match loop_value_count {
                        0 => {
                            self.return_stack.pop_frame();
                            self.return_stack.push(Empty);
                        }
                        1 => {
                            self.return_stack.pop_frame_and_keep_results();
                        }
                        _ => {
                            self.return_stack.pop_frame_and_keep_results();
                        }
                    }
                }
                Range { min, max } => {
                    for i in min..max {
                        self.return_stack.push(Number(i as f64))
                    }
                }
                Empty => {}
                _ => {
                    self.return_stack.push(value.clone());
                }
            }
        } else {
            self.return_stack.pop_frame_and_keep_results();
        }

        Ok(())
    }

    fn evaluate(&mut self, node: &AstNode) -> RuntimeResult {
        runtime_trace!(self, "evaluate - {}", node.node);

        self.return_stack.start_frame();

        use Value::*;

        match &node.node {
            Node::Bool(b) => {
                self.return_stack.push(Bool(*b));
            }
            Node::Number(n) => {
                self.return_stack.push(Number(*n));
            }
            Node::Vec4(v) => {
                self.return_stack.push(Vec4(*v));
            }
            Node::Str(s) => {
                self.return_stack.push(Str(s.clone()));
            }
            Node::List(elements) => {
                self.evaluate_expressions(elements)?;
                if self.return_stack.values().len() == 1 {
                    let value = self.return_stack.value().clone();
                    self.return_stack.pop_frame();
                    match value {
                        List(_) => self.return_stack.push(value),
                        _ => self.return_stack.push(List(Rc::new(vec![value]))),
                    }
                } else {
                    // TODO check values in return stack for unexpanded for loops + ranges
                    let list = self
                        .return_stack
                        .values()
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>();
                    self.return_stack.pop_frame();
                    self.return_stack.push(Value::List(Rc::new(list)));
                }
            }
            Node::Range {
                min,
                inclusive,
                max,
            } => {
                self.evaluate(min)?;
                let min = self.return_stack.value().clone();
                self.return_stack.pop_frame();

                self.evaluate(max)?;
                let max = self.return_stack.value().clone();
                self.return_stack.pop_frame();

                match (min, max) {
                    (Number(min), Number(max)) => {
                        let min = min as isize;
                        let max = max as isize;
                        let max = if *inclusive { max + 1 } else { max };
                        if min <= max {
                            self.return_stack.push(Range { min, max });
                        } else {
                            return runtime_error!(
                                node,
                                "Invalid range, min should be less than or equal to max - min: {}, max: {}",
                                min,
                                max);
                        }
                    }
                    unexpected => {
                        return runtime_error!(
                            node,
                            "Expected numbers for range bounds, found min: {}, max: {}",
                            unexpected.0,
                            unexpected.1
                        )
                    }
                }
            }
            Node::Map(entries) => {
                let mut map = HashMap::new();
                for (id, node) in entries.iter() {
                    self.evaluate_and_capture(node)?;
                    map.insert(id.clone(), self.return_stack.value().clone());
                    self.return_stack.pop_frame();
                }
                self.return_stack.push(Map(Rc::new(map)));
            }
            Node::Index { id, expression } => {
                self.list_index(id, expression, node)?;
            }
            Node::Id(id) => {
                self.return_stack.push(self.get_value_or_error(id, node)?);
            }
            Node::Block(block) => {
                self.evaluate_block(&block)?;
                self.return_stack.pop_frame_and_keep_results();
            }
            Node::Expressions(expressions) => {
                self.evaluate_expressions(&expressions)?;
                self.return_stack.pop_frame_and_keep_results();
            }
            Node::Function(f) => self.return_stack.push(Function(f.clone())),
            Node::Call { function, args } => {
                return self.call_function(function, args, node);
            }
            Node::Assign {
                id,
                expression,
                global,
            } => {
                self.evaluate_and_capture(expression)?;

                let value = self.return_stack.value().clone();
                self.return_stack.pop_frame();

                runtime_trace!(self, "Assigning to {}: {}", id, value);

                self.set_value(id, &value, *global);
                self.return_stack.push(value);
            }
            Node::MultiAssign {
                ids,
                expressions,
                global,
            } => {
                if expressions.len() == 1 {
                    self.evaluate_and_capture(expressions.first().unwrap())?;
                    let value = self.return_stack.value().clone();
                    self.return_stack.pop_frame_and_keep_results();

                    match value {
                        List(l) => {
                            let mut result_iter = l.iter();
                            for id in ids.iter() {
                                let value = result_iter.next().unwrap_or(&Empty);
                                self.set_value(id, &value, *global);
                            }
                        }
                        _ => {
                            self.set_value(ids.first().unwrap(), &value, *global);

                            for id in ids[1..].iter() {
                                self.set_value(id, &Empty, *global);
                            }
                        }
                    }
                } else {
                    for expression in expressions.iter() {
                        self.evaluate_and_capture(expression)?;
                        self.return_stack.pop_frame_and_keep_results();
                    }

                    let results = self.return_stack.values().to_owned();

                    match results.as_slice() {
                        [] => unreachable!(),
                        [single_value] => {
                            self.set_value(ids.first().unwrap(), &single_value, *global);
                            // set remaining ids to empty
                            for id in ids[1..].iter() {
                                self.set_value(id, &Empty, *global);
                            }
                        }
                        _ => {
                            let mut result_iter = results.iter();
                            for id in ids.iter() {
                                let value = result_iter.next().unwrap_or(&Empty);
                                self.set_value(id, &value, *global);
                            }
                        }
                    }
                }
            }
            Node::Op { op, lhs, rhs } => {
                self.evaluate(lhs)?;
                let a = self.return_stack.value().clone();
                self.return_stack.pop_frame();

                self.evaluate(rhs)?;
                let b = self.return_stack.value().clone();
                self.return_stack.pop_frame();

                macro_rules! binary_op_error {
                    ($op:ident, $a:ident, $b:ident) => {
                        runtime_error!(
                            node,
                            "Unable to perform operation {:?} with lhs: '{}' and rhs: '{}'",
                            op,
                            a,
                            b
                        )
                    };
                };

                let result = match op {
                    AstOp::Equal => Ok((a == b).into()),
                    AstOp::NotEqual => Ok((a != b).into()),
                    _ => match (&a, &b) {
                        (Number(a), Number(b)) => match op {
                            AstOp::Add => Ok(Number(a + b)),
                            AstOp::Subtract => Ok(Number(a - b)),
                            AstOp::Multiply => Ok(Number(a * b)),
                            AstOp::Divide => Ok(Number(a / b)),
                            AstOp::Modulo => Ok(Number(a % b)),
                            AstOp::Less => Ok(Bool(a < b)),
                            AstOp::LessOrEqual => Ok(Bool(a <= b)),
                            AstOp::Greater => Ok(Bool(a > b)),
                            AstOp::GreaterOrEqual => Ok(Bool(a >= b)),
                            _ => binary_op_error!(op, a, b),
                        },
                        (Vec4(a), Vec4(b)) => match op {
                            AstOp::Add => Ok(Vec4(*a + *b)),
                            AstOp::Subtract => Ok(Vec4(*a - *b)),
                            AstOp::Multiply => Ok(Vec4(*a * *b)),
                            AstOp::Divide => Ok(Vec4(*a / *b)),
                            AstOp::Modulo => Ok(Vec4(*a % *b)),
                            _ => binary_op_error!(op, a, b),
                        },
                        (Number(a), Vec4(b)) => match op {
                            AstOp::Add => Ok(Vec4(*a + *b)),
                            AstOp::Subtract => Ok(Vec4(*a - *b)),
                            AstOp::Multiply => Ok(Vec4(*a * *b)),
                            AstOp::Divide => Ok(Vec4(*a / *b)),
                            AstOp::Modulo => Ok(Vec4(*a % *b)),
                            _ => binary_op_error!(op, a, b),
                        },
                        (Vec4(a), Number(b)) => match op {
                            AstOp::Add => Ok(Vec4(*a + *b)),
                            AstOp::Subtract => Ok(Vec4(*a - *b)),
                            AstOp::Multiply => Ok(Vec4(*a * *b)),
                            AstOp::Divide => Ok(Vec4(*a / *b)),
                            AstOp::Modulo => Ok(Vec4(*a % *b)),
                            _ => binary_op_error!(op, a, b),
                        },
                        (Bool(a), Bool(b)) => match op {
                            AstOp::And => Ok(Bool(*a && *b)),
                            AstOp::Or => Ok(Bool(*a || *b)),
                            _ => binary_op_error!(op, a, b),
                        },
                        (List(a), List(b)) => match op {
                            AstOp::Add => {
                                let mut result = Vec::clone(a);
                                result.extend(Vec::clone(b).into_iter());
                                Ok(List(Rc::new(result)))
                            }
                            _ => binary_op_error!(op, a, b),
                        },
                        (Map(a), Map(b)) => match op {
                            AstOp::Add => {
                                let mut result = HashMap::clone(a);
                                result.extend(HashMap::clone(b).into_iter());
                                Ok(Map(Rc::new(result)))
                            }
                            _ => binary_op_error!(op, a, b),
                        },
                        _ => binary_op_error!(op, a, b),
                    },
                }?;

                self.return_stack.push(result);
            }
            Node::If {
                condition,
                then_node,
                else_if_condition,
                else_if_node,
                else_node,
            } => {
                self.evaluate(condition)?;
                let maybe_bool = self.return_stack.value().clone();
                self.return_stack.pop_frame();

                if let Bool(condition_value) = maybe_bool {
                    if condition_value {
                        self.evaluate(then_node)?;
                        self.return_stack.pop_frame_and_keep_results();
                        return Ok(());
                    }

                    if else_if_condition.is_some() {
                        self.evaluate(&else_if_condition.as_ref().unwrap())?;
                        let maybe_bool = self.return_stack.value().clone();
                        self.return_stack.pop_frame();

                        if let Bool(condition_value) = maybe_bool {
                            if condition_value {
                                self.evaluate(else_if_node.as_ref().unwrap())?;
                                self.return_stack.pop_frame_and_keep_results();
                                return Ok(());
                            }
                        } else {
                            return runtime_error!(
                                node,
                                "Expected bool in else if statement, found {}",
                                maybe_bool
                            );
                        }
                    }

                    if else_node.is_some() {
                        self.evaluate(else_node.as_ref().unwrap())?;
                        self.return_stack.pop_frame_and_keep_results();
                    }
                } else {
                    return runtime_error!(
                        node,
                        "Expected bool in if statement, found {}",
                        maybe_bool
                    );
                }
            }
            Node::For(f) => {
                self.return_stack.push(For(f.clone()));
            }
        }

        Ok(())
    }

    fn set_value(&mut self, id: &Id, value: &Value, global: bool) {
        if self.call_stack.frame() == 0 || global {
            self.global.values.insert(id.clone(), value.clone());
        } else {
            if let Some(exists) = self.call_stack.get_mut(id.as_ref()) {
                *exists = value.clone();
            } else {
                self.call_stack.extend(id.clone(), value.clone());
            }
        }
    }

    fn get_value(&self, lookup_id: &LookupId) -> Option<Value> {
        macro_rules! value_or_map_lookup {
            ($value:expr) => {{
                if lookup_id.0.len() == 1 {
                    $value
                } else if $value.is_some() {
                    lookup_id.0[1..]
                        .iter()
                        .try_fold($value.unwrap(), |result, id| {
                            match result {
                                Value::Map(data) => data.get(id),
                                _unexpected => None, // TODO error, previous item wasn't a map
                            }
                        })
                } else {
                    None
                }
            }};
        }

        if self.call_stack.frame() > 0 {
            let value = self.call_stack.get(lookup_id.0.first().unwrap());
            if let Some(value) = value_or_map_lookup!(value) {
                return Some(value.clone());
            }
        }

        let global_value = self.global.values.get(lookup_id.0.first().unwrap());
        value_or_map_lookup!(global_value).map(|v| v.clone())
    }

    fn get_value_or_error(&self, id: &LookupId, node: &AstNode) -> Result<Value, Error> {
        match self.get_value(id) {
            Some(v) => Ok(v),
            None => runtime_error!(node, "Value '{}' not found", id),
        }
    }

    fn run_for_loop(&mut self, for_statement: &Value, node: &AstNode) -> RuntimeResult {
        runtime_trace!(self, "run_for_loop");
        use Value::*;

        self.return_stack.start_frame();

        if let For(f) = for_statement {
            let iter = MultiRangeValueIterator(
                f.ranges
                    .iter()
                    .map(|range| {
                        self.evaluate(range)?;
                        let range = self.return_stack.value().clone();
                        self.return_stack.pop_frame();

                        match range {
                            v @ List(_) | v @ Range { .. } => Ok(ValueIterator::new(v)),
                            unexpected => runtime_error!(
                                node,
                                "Expected iterable range in for statement, found {}",
                                unexpected
                            ),
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            );

            let single_range = f.ranges.len() == 1;
            for values in iter {
                let mut arg_iter = f.args.iter().peekable();
                for value in values.iter() {
                    match value {
                        List(a) if single_range => {
                            for list_value in a.iter() {
                                match arg_iter.next() {
                                    Some(arg) => self.set_value(arg, &list_value, false), // TODO
                                    None => break,
                                }
                            }
                        }
                        _ => self.set_value(
                            arg_iter
                                .next()
                                .expect("For loops have at least one argument"),
                            &value,
                            false,
                        ),
                    }
                }
                for remaining_arg in arg_iter {
                    self.set_value(remaining_arg, &Value::Empty, false);
                }

                if let Some(condition) = &f.condition {
                    self.evaluate(&condition)?;
                    let value = self.return_stack.value().clone();
                    self.return_stack.pop_frame();

                    match value {
                        Bool(b) => {
                            if !b {
                                continue;
                            }
                        }
                        unexpected => {
                            return runtime_error!(
                                node,
                                "Expected bool in for statement condition, found {}",
                                unexpected
                            )
                        }
                    }
                }
                self.evaluate_and_capture(&f.body)?;
                self.return_stack.pop_frame_and_keep_results();
            }
        }

        Ok(())
    }

    fn list_index(&mut self, id: &LookupId, expression: &AstNode, node: &AstNode) -> RuntimeResult {
        use Value::*;

        self.evaluate(expression)?;
        let index = self.return_stack.value().clone();
        self.return_stack.pop_frame();

        let maybe_list = self.get_value_or_error(id, node)?;

        if let List(elements) = maybe_list {
            match index {
                Number(i) => {
                    let i = i as usize;
                    if i < elements.len() {
                        self.return_stack.push(elements[i].clone());
                    } else {
                        return runtime_error!(
                            node,
                            "Index out of bounds: '{}' has a length of {} but the index is {}",
                            id,
                            elements.len(),
                            i
                        );
                    }
                }
                Range { min, max } => {
                    let umin = min as usize;
                    let umax = max as usize;
                    if min < 0 || max < 0 {
                        return runtime_error!(
                            node,
                            "Indexing with negative indices isn't supported, min: {}, max: {}",
                            min,
                            max
                        );
                    } else if umin >= elements.len() || umax >= elements.len() {
                        return runtime_error!(
                            node,
                            "Index out of bounds: '{}' has a length of {} - min: {}, max: {}",
                            id,
                            elements.len(),
                            min,
                            max
                        );
                    } else {
                        // TODO Avoid allocating new vec, introduce 'slice' value type
                        self.return_stack.push(List(Rc::new(
                            elements[umin..umax].iter().cloned().collect::<Vec<_>>(),
                        )));
                    }
                }
                _ => {
                    return runtime_error!(
                        node,
                        "Indexing is only supported with number values or ranges, found {})",
                        index
                    )
                }
            }
        } else {
            return runtime_error!(
                node,
                "Indexing is only supported for Lists, found {}",
                maybe_list
            );
        }

        Ok(())
    }

    fn call_function(
        &mut self,
        id: &LookupId,
        args: &Vec<AstNode>,
        node: &AstNode,
    ) -> RuntimeResult {
        use Value::*;

        runtime_trace!(self, "call_function - {}", id);

        let maybe_function = match self.get_value(id) {
            Some(Function(f)) => Some(f.clone()),
            Some(unexpected) => {
                return runtime_error!(
                    node,
                    "Expected function for value {}, found {}",
                    id,
                    unexpected
                )
            }
            None => None,
        };

        if let Some(f) = maybe_function {
            let arg_count = f.args.len();
            let expected_args =
                if id.0.len() > 1 && arg_count > 0 && f.args.first().unwrap().as_ref() == "self" {
                    arg_count - 1
                } else {
                    arg_count
                };

            if args.len() != expected_args {
                return runtime_error!(
                    node,
                    "Incorrect argument count while calling '{}': expected {}, found {} - {:?}",
                    id,
                    expected_args,
                    args.len(),
                    f.args
                );
            }

            // allow the function that's being called to call itself
            self.call_stack
                .push(id.0.first().unwrap().clone(), Function(f.clone()));

            // implicit self for map functions
            if id.0.len() > 1 {
                match f.args.first() {
                    Some(self_arg) if self_arg.as_ref() == "self" => {
                        // TODO id slices
                        let mut map_id = id.0.clone();
                        map_id.pop();
                        let map = self.get_value(&LookupId(map_id)).unwrap();
                        self.call_stack.push(self_arg.clone(), map);
                    }
                    _ => {}
                }
            }

            for (name, arg) in f.args.iter().zip(args.iter()) {
                let expression_result = self.evaluate_and_capture(arg);
                let arg_value = self.return_stack.value().clone();
                self.return_stack.pop_frame();

                self.call_stack.push(name.clone(), arg_value);

                if expression_result.is_err() {
                    self.call_stack.cancel();
                    return expression_result;
                }
            }

            self.call_stack.commit();
            let result = self.evaluate_block(&f.body);
            self.return_stack.pop_frame_and_keep_results();
            self.call_stack.pop_frame();

            return result;
        }

        self.evaluate_expressions(args)?;

        if let Some(value) = self.builtins.get_mut(&id.0) {
            return match value {
                BuiltinValue::Function(f) => {
                    let builtin_result = f(&self.return_stack.values());
                    self.return_stack.pop_frame();
                    match builtin_result {
                        Ok(v) => {
                            self.return_stack.push(v);
                            Ok(())
                        }
                        Err(e) => runtime_error!(node, e),
                    }
                }
                unexpected => {
                    self.return_stack.pop_frame();
                    runtime_error!(node, "Expected function for '{}', found {}", id, unexpected)
                }
            };
        }

        self.return_stack.pop_frame();

        runtime_error!(node, "Function '{}' not found", id)
    }

    #[allow(dead_code)]
    fn runtime_indent(&self) -> String {
        " ".repeat(self.return_stack.frame_count())
    }
}
