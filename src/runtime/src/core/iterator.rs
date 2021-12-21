pub mod adaptors;

use {
    super::{num2::num2_from_iterator, num4::num4_from_iterator},
    crate::{
        runtime_error, unexpected_type_error_with_slice,
        value_iterator::{ValueIterator, ValueIteratorOutput as Output},
        BinaryOp, CallArgs, DataMap, RuntimeError, RuntimeResult, Value, ValueList, ValueMap,
        ValueVec, Vm,
    },
};

pub fn make_module() -> ValueMap {
    use Value::*;

    let mut result = ValueMap::new();

    result.add_fn("all", |vm, args| match vm.get_args(args) {
        [iterable, predicate] if iterable.is_iterable() && predicate.is_callable() => {
            let iterable = iterable.clone();
            let predicate = predicate.clone();

            for output in vm.make_iterator(iterable)? {
                let predicate_result = match output {
                    Output::Value(value) => {
                        vm.run_function(predicate.clone(), CallArgs::Single(value))
                    }
                    Output::ValuePair(a, b) => {
                        vm.run_function(predicate.clone(), CallArgs::AsTuple(&[a, b]))
                    }
                    Output::Error(error) => return Err(error),
                };

                match predicate_result {
                    Ok(Bool(result)) => {
                        if !result {
                            return Ok(Bool(false));
                        }
                    }
                    Ok(unexpected) => {
                        return unexpected_type_error_with_slice(
                            "iterator.all",
                            "a Bool to be returned from the predicate",
                            &[unexpected],
                        )
                    }
                    Err(error) => return Err(error.with_prefix("iterator.all")),
                }
            }

            Ok(Bool(true))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.all",
            "an iterable value and predicate Function as arguments",
            unexpected,
        ),
    });

    result.add_fn("any", |vm, args| match vm.get_args(args) {
        [iterable, predicate] if iterable.is_iterable() && predicate.is_callable() => {
            let iterable = iterable.clone();
            let predicate = predicate.clone();

            for output in vm.make_iterator(iterable)? {
                let predicate_result = match output {
                    Output::Value(value) => {
                        vm.run_function(predicate.clone(), CallArgs::Single(value))
                    }
                    Output::ValuePair(a, b) => {
                        vm.run_function(predicate.clone(), CallArgs::AsTuple(&[a, b]))
                    }
                    Output::Error(error) => return Err(error),
                };

                match predicate_result {
                    Ok(Bool(result)) => {
                        if result {
                            return Ok(Bool(true));
                        }
                    }
                    Ok(unexpected) => {
                        return unexpected_type_error_with_slice(
                            "iterator.any",
                            "a Bool to be returned from the predicate",
                            &[unexpected],
                        )
                    }
                    Err(error) => return Err(error.with_prefix("iterator.all")),
                }
            }

            Ok(Bool(false))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.any",
            "an iterable value and predicate Function as arguments",
            unexpected,
        ),
    });

    result.add_fn("chain", |vm, args| match vm.get_args(args) {
        [iterable_a, iterable_b] if iterable_a.is_iterable() && iterable_b.is_iterable() => {
            let iterable_a = iterable_a.clone();
            let iterable_b = iterable_b.clone();
            let result = ValueIterator::make_external(adaptors::Chain::new(
                vm.make_iterator(iterable_a)?,
                vm.make_iterator(iterable_b)?,
            ));

            Ok(Iterator(result))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.chain",
            "two iterable values as arguments",
            unexpected,
        ),
    });

    result.add_fn("chunks", |vm, args| match vm.get_args(args) {
        [iterable, Number(n)] if iterable.is_sequence() && *n >= 1 => {
            let iterable = iterable.clone();
            let n = *n;
            let result = adaptors::Chunks::new(vm.make_iterator(iterable)?, n.into());
            Ok(Iterator(ValueIterator::make_external(result)))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.chunks",
            "a value with a range (like a List or String), \
             and a chunk size greater than zero as arguments",
            unexpected,
        ),
    });

    result.add_fn("consume", |vm, args| match vm.get_args(args) {
        [iterable] if iterable.is_iterable() => {
            let iterable = iterable.clone();
            for output in vm.make_iterator(iterable)? {
                if let Output::Error(error) = output {
                    return Err(error);
                }
            }
            Ok(Empty)
        }
        [iterable, f] if iterable.is_iterable() && f.is_callable() => {
            let iterable = iterable.clone();
            let f = f.clone();
            for output in vm.make_iterator(iterable)? {
                let run_result = match output {
                    Output::Value(value) => vm.run_function(f.clone(), CallArgs::Single(value)),
                    Output::ValuePair(a, b) => {
                        vm.run_function(f.clone(), CallArgs::AsTuple(&[a, b]))
                    }
                    Output::Error(error) => return Err(error),
                };

                if run_result.is_err() {
                    return run_result;
                }
            }
            Ok(Empty)
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.consume",
            "an Iterable Value (and optional Function) as arguments",
            unexpected,
        ),
    });

    result.add_fn("copy", |vm, args| match vm.get_args(args) {
        [Iterator(iter)] => Ok(Iterator(iter.make_copy())),
        unexpected => {
            unexpected_type_error_with_slice("iterator.copy", "an Iterator as argument", unexpected)
        }
    });

    result.add_fn("count", |vm, args| match vm.get_args(args) {
        [iterable] if iterable.is_iterable() => {
            let iterable = iterable.clone();
            let mut result = 0;
            for output in vm.make_iterator(iterable)? {
                if let Output::Error(error) = output {
                    return Err(error);
                }
                result += 1;
            }
            Ok(Number(result.into()))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.count",
            "an iterable value as argument",
            unexpected,
        ),
    });

    result.add_fn("each", |vm, args| match vm.get_args(args) {
        [iterable, f] if iterable.is_iterable() && f.is_callable() => {
            let iterable = iterable.clone();
            let f = f.clone();
            let result = adaptors::Each::new(vm.make_iterator(iterable)?, f, vm.spawn_shared_vm());

            Ok(Iterator(ValueIterator::make_external(result)))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.each",
            "an iterable value and a Function as arguments",
            unexpected,
        ),
    });

    result.add_fn("cycle", |vm, args| match vm.get_args(args) {
        [iterable] if iterable.is_iterable() => {
            let iterable = iterable.clone();
            let result = adaptors::Cycle::new(vm.make_iterator(iterable)?);

            Ok(Iterator(ValueIterator::make_external(result)))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.cycle",
            "an iterable value as argument",
            unexpected,
        ),
    });

    result.add_fn("enumerate", |vm, args| match vm.get_args(args) {
        [iterable] if iterable.is_iterable() => {
            let iterable = iterable.clone();
            let result = adaptors::Enumerate::new(vm.make_iterator(iterable)?);
            Ok(Iterator(ValueIterator::make_external(result)))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.enumerate",
            "an iterable value as argument",
            unexpected,
        ),
    });

    result.add_fn("find", |vm, args| match vm.get_args(args) {
        [iterable, predicate] if iterable.is_iterable() && predicate.is_callable() => {
            let iterable = iterable.clone();
            let predicate = predicate.clone();

            for output in vm.make_iterator(iterable)?.map(collect_pair) {
                match output {
                    Output::Value(value) => {
                        match vm.run_function(predicate.clone(), CallArgs::Single(value.clone())) {
                            Ok(Bool(result)) => {
                                if result {
                                    return Ok(value);
                                }
                            }
                            Ok(unexpected) => {
                                return unexpected_type_error_with_slice(
                                    "iterator.find",
                                    "a Bool to be returned from the predicate",
                                    &[unexpected],
                                )
                            }
                            Err(error) => return Err(error.with_prefix("iterator.find")),
                        }
                    }
                    Output::Error(error) => return Err(error),
                    _ => unreachable!(),
                }
            }

            Ok(Empty)
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.find",
            "an iterable value and a predicate Function as arguments",
            unexpected,
        ),
    });

    result.add_fn("flatten", |vm, args| match vm.get_args(args) {
        [iterable] if iterable.is_iterable() => {
            let iterable = iterable.clone();
            let result = adaptors::Flatten::new(vm.make_iterator(iterable)?, vm.spawn_shared_vm());

            Ok(Iterator(ValueIterator::make_external(result)))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.cycle",
            "an iterable value as argument",
            unexpected,
        ),
    });

    result.add_fn("fold", |vm, args| {
        match vm.get_args(args) {
            [iterable, result, f] if iterable.is_iterable() && f.is_callable() => {
                let iterable = iterable.clone();
                let result = result.clone();
                let f = f.clone();
                let mut iter = vm.make_iterator(iterable)?;

                match iter
                    .borrow_internals(|iterator| {
                        let mut fold_result = result.clone();
                        for value in iterator.map(collect_pair) {
                            match value {
                                Output::Value(value) => {
                                    match vm.run_function(
                                        f.clone(),
                                        CallArgs::Separate(&[fold_result, value]),
                                    ) {
                                        Ok(result) => fold_result = result,
                                        Err(error) => {
                                            return Some(Output::Error(
                                                error.with_prefix("iterator.fold"),
                                            ))
                                        }
                                    }
                                }
                                Output::Error(error) => return Some(Output::Error(error)),
                                _ => unreachable!(),
                            }
                        }

                        Some(Output::Value(fold_result))
                    })
                    // None is never returned from the closure
                    .unwrap()
                {
                    Output::Value(result) => Ok(result),
                    Output::Error(error) => Err(error),
                    _ => unreachable!(),
                }
            }
            unexpected => unexpected_type_error_with_slice(
                "iterator.fold",
                "an iterable value, initial value, and folding Function as arguments",
                unexpected,
            ),
        }
    });

    result.add_fn("intersperse", |vm, args| match vm.get_args(args) {
        [iterable, separator_fn] if iterable.is_iterable() && separator_fn.is_callable() => {
            let iterable = iterable.clone();
            let separator_fn = separator_fn.clone();
            let result = adaptors::IntersperseWith::new(
                vm.make_iterator(iterable)?,
                separator_fn,
                vm.spawn_shared_vm(),
            );

            Ok(Iterator(ValueIterator::make_external(result)))
        }
        [iterable, separator] if iterable.is_iterable() => {
            let iterable = iterable.clone();
            let separator = separator.clone();
            let result = adaptors::Intersperse::new(vm.make_iterator(iterable)?, separator);

            Ok(Iterator(ValueIterator::make_external(result)))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.intersperse",
            "an iterable value and separator as arguments",
            unexpected,
        ),
    });

    result.add_fn("iter", |vm, args| match vm.get_args(args) {
        [iterable] if iterable.is_iterable() => {
            let iterable = iterable.clone();
            Ok(Iterator(vm.make_iterator(iterable)?))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.iter",
            "an iterable value as argument",
            unexpected,
        ),
    });

    result.add_fn("keep", |vm, args| match vm.get_args(args) {
        [iterable, predicate] if iterable.is_iterable() && predicate.is_callable() => {
            let iterable = iterable.clone();
            let predicate = predicate.clone();
            let result =
                adaptors::Keep::new(vm.make_iterator(iterable)?, predicate, vm.spawn_shared_vm());
            Ok(Iterator(ValueIterator::make_external(result)))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.keep",
            "an iterable value and a predicate Function as arguments",
            unexpected,
        ),
    });

    result.add_fn("last", |vm, args| match vm.get_args(args) {
        [iterable] if iterable.is_iterable() => {
            let iterable = iterable.clone();
            let mut result = Empty;

            let mut iter = vm.make_iterator(iterable)?.map(collect_pair);
            for output in &mut iter {
                match output {
                    Output::Value(value) => result = value,
                    Output::Error(error) => return Err(error),
                    _ => unreachable!(),
                }
            }

            Ok(result)
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.last",
            "an iterable value as argument",
            unexpected,
        ),
    });

    result.add_fn("max", |vm, args| match vm.get_args(args) {
        [iterable] if iterable.is_iterable() => {
            let iterable = iterable.clone();
            run_iterator_comparison(vm, iterable, InvertResult::Yes)
                .map_err(|e| e.with_prefix("iterator.max"))
        }
        [iterable, key_fn] if iterable.is_iterable() && key_fn.is_callable() => {
            let iterable = iterable.clone();
            let key_fn = key_fn.clone();
            run_iterator_comparison_by_key(vm, iterable, key_fn, InvertResult::Yes)
                .map_err(|e| e.with_prefix("iterator.max"))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.max",
            "an iterable value and an optional key function as arguments",
            unexpected,
        ),
    });

    result.add_fn("min", |vm, args| match vm.get_args(args) {
        [iterable] if iterable.is_iterable() => {
            let iterable = iterable.clone();
            run_iterator_comparison(vm, iterable, InvertResult::No)
                .map_err(|e| e.with_prefix("iterator.min"))
        }
        [iterable, key_fn] if iterable.is_iterable() && key_fn.is_callable() => {
            let iterable = iterable.clone();
            let key_fn = key_fn.clone();
            run_iterator_comparison_by_key(vm, iterable, key_fn, InvertResult::No)
                .map_err(|e| e.with_prefix("iterator.min"))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.min",
            "an iterable value and an optional key function as arguments",
            unexpected,
        ),
    });

    result.add_fn("min_max", |vm, args| match vm.get_args(args) {
        [iterable] if iterable.is_iterable() => {
            let iterable = iterable.clone();
            let mut result = None;

            for iter_output in vm.make_iterator(iterable)?.map(collect_pair) {
                match iter_output {
                    Output::Value(value) => {
                        result = Some(match result {
                            Some((min, max)) => (
                                compare_values(vm, min, value.clone(), InvertResult::No)
                                    .map_err(|e| e.with_prefix("iterator.min_max"))?,
                                compare_values(vm, max, value, InvertResult::Yes)
                                    .map_err(|e| e.with_prefix("iterator.min_max"))?,
                            ),
                            None => (value.clone(), value),
                        })
                    }
                    Output::Error(error) => return Err(error),
                    _ => unreachable!(),
                }
            }

            Ok(result.map_or(Empty, |(min, max)| Tuple(vec![min, max].into())))
        }
        [iterable, key_fn] if iterable.is_iterable() && key_fn.is_callable() => {
            let iterable = iterable.clone();
            let key_fn = key_fn.clone();
            let mut result = None;

            for iter_output in vm.make_iterator(iterable)?.map(collect_pair) {
                match iter_output {
                    Output::Value(value) => {
                        let key =
                            vm.run_function(key_fn.clone(), CallArgs::Single(value.clone()))?;
                        let value_and_key = (value, key);

                        result = Some(match result {
                            Some((min_and_key, max_and_key)) => (
                                compare_values_with_key(
                                    vm,
                                    min_and_key,
                                    value_and_key.clone(),
                                    InvertResult::No,
                                )
                                .map_err(|e| e.with_prefix("iterator.min_max"))?,
                                compare_values_with_key(
                                    vm,
                                    max_and_key,
                                    value_and_key,
                                    InvertResult::Yes,
                                )
                                .map_err(|e| e.with_prefix("iterator.min_max"))?,
                            ),
                            None => (value_and_key.clone(), value_and_key),
                        })
                    }
                    Output::Error(error) => return Err(error),
                    _ => unreachable!(), // value pairs have been collected in collect_pair
                }
            }

            Ok(result.map_or(Empty, |((min, _), (max, _))| Tuple(vec![min, max].into())))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.min_max",
            "an iterable value and an optional key function as arguments",
            unexpected,
        ),
    });

    result.add_fn("next", |vm, args| match vm.get_args(args) {
        [Iterator(i)] => match i.clone().next().map(collect_pair) {
            Some(Output::Value(value)) => Ok(value),
            Some(Output::Error(error)) => Err(error),
            None => Ok(Value::Empty),
            _ => unreachable!(),
        },
        unexpected => {
            unexpected_type_error_with_slice("iterator.next", "an Iterator as argument", unexpected)
        }
    });

    result.add_fn("position", |vm, args| match vm.get_args(args) {
        [iterable, predicate] if iterable.is_iterable() && predicate.is_callable() => {
            let iterable = iterable.clone();
            let predicate = predicate.clone();

            for (i, output) in vm.make_iterator(iterable)?.enumerate() {
                let predicate_result = match output {
                    Output::Value(value) => {
                        vm.run_function(predicate.clone(), CallArgs::Single(value))
                    }
                    Output::ValuePair(a, b) => {
                        vm.run_function(predicate.clone(), CallArgs::AsTuple(&[a, b]))
                    }
                    Output::Error(error) => return Err(error),
                };

                match predicate_result {
                    Ok(Bool(result)) => {
                        if result {
                            return Ok(Number(i.into()));
                        }
                    }
                    Ok(unexpected) => {
                        return unexpected_type_error_with_slice(
                            "iterator.position",
                            "a Bool to be returned from the predicate",
                            &[unexpected],
                        )
                    }
                    Err(error) => return Err(error.with_prefix("iterator.position")),
                }
            }

            Ok(Empty)
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.position",
            "an iterable value and a predicate Function as arguments",
            unexpected,
        ),
    });

    result.add_fn("product", |vm, args| {
        let (iterable, initial_value) = match vm.get_args(args) {
            [iterable] if iterable.is_iterable() => (iterable.clone(), Value::Number(1.into())),
            [iterable, initial_value] if iterable.is_iterable() => {
                (iterable.clone(), initial_value.clone())
            }
            unexpected => {
                return unexpected_type_error_with_slice(
                    "iterator.product",
                    "an iterable value and optional initial value as arguments",
                    unexpected,
                )
            }
        };

        fold_with_operator(vm, iterable, initial_value, BinaryOp::Multiply)
            .map_err(|e| e.with_prefix("iterator.product"))
    });

    result.add_fn("skip", |vm, args| match vm.get_args(args) {
        [iterable, Number(n)] if iterable.is_iterable() && *n >= 0.0 => {
            let iterable = iterable.clone();
            let n = *n;
            let mut iter = vm.make_iterator(iterable)?;

            for _ in 0..n.into() {
                if let Some(Output::Error(error)) = iter.next() {
                    return Err(error);
                }
            }

            Ok(Iterator(iter))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.skip",
            "an iterable value and non-negative number as arguments",
            unexpected,
        ),
    });

    result.add_fn("sum", |vm, args| {
        let (iterable, initial_value) = match vm.get_args(args) {
            [iterable] if iterable.is_iterable() => (iterable.clone(), Value::Number(0.into())),
            [iterable, initial_value] if iterable.is_iterable() => {
                (iterable.clone(), initial_value.clone())
            }
            unexpected => {
                return unexpected_type_error_with_slice(
                    "iterator.sum",
                    "an iterable value and optional initial value as arguments",
                    unexpected,
                )
            }
        };

        fold_with_operator(vm, iterable, initial_value, BinaryOp::Add)
            .map_err(|e| e.with_prefix("iterator.sum"))
    });

    result.add_fn("take", |vm, args| match vm.get_args(args) {
        [iterable, Number(n)] if iterable.is_iterable() && *n >= 0.0 => {
            let iterable = iterable.clone();
            let n = *n;
            let result = adaptors::Take::new(vm.make_iterator(iterable)?, n.into());
            Ok(Iterator(ValueIterator::make_external(result)))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.take",
            "an iterable value and non-negative number as arguments",
            unexpected,
        ),
    });

    result.add_fn("to_list", |vm, args| match vm.get_args(args) {
        [iterable] if iterable.is_iterable() => {
            let iterable = iterable.clone();
            let iterator = vm.make_iterator(iterable)?;
            let (size_hint, _) = iterator.size_hint();
            let mut result = ValueVec::with_capacity(size_hint);

            for output in iterator.map(collect_pair) {
                match output {
                    Output::Value(value) => result.push(value),
                    Output::Error(error) => return Err(error),
                    _ => unreachable!(),
                }
            }

            Ok(List(ValueList::with_data(result)))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.to_list",
            "an iterable value as argument",
            unexpected,
        ),
    });

    result.add_fn("to_map", |vm, args| match vm.get_args(args) {
        [iterable] if iterable.is_iterable() => {
            let iterable = iterable.clone();
            let iterator = vm.make_iterator(iterable)?;
            let (size_hint, _) = iterator.size_hint();
            let mut result = DataMap::with_capacity(size_hint);

            for output in iterator {
                match output {
                    Output::ValuePair(key, value) => {
                        result.insert(key.into(), value);
                    }
                    Output::Value(Tuple(t)) if t.data().len() == 2 => {
                        let key = t.data()[0].clone();
                        let value = t.data()[1].clone();
                        result.insert(key.into(), value);
                    }
                    Output::Value(value) => {
                        result.insert(value.into(), Value::Empty);
                    }
                    Output::Error(error) => return Err(error),
                }
            }

            Ok(Map(ValueMap::with_data(result)))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.to_map",
            "an iterable value as argument",
            unexpected,
        ),
    });

    result.add_fn("to_num2", |vm, args| match vm.get_args(args) {
        [iterable] if iterable.is_iterable() => {
            let iterable = iterable.clone();
            let iterator = vm.make_iterator(iterable)?;
            Ok(Num2(num2_from_iterator(iterator, "iterator.to_num2")?))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.to_num2",
            "an iterable value as argument",
            unexpected,
        ),
    });

    result.add_fn("to_num4", |vm, args| match vm.get_args(args) {
        [iterable] if iterable.is_iterable() => {
            let iterable = iterable.clone();
            let iterator = vm.make_iterator(iterable)?;
            Ok(Num4(num4_from_iterator(iterator, "iterator.to_num4")?))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.to_num4",
            "an iterable value as argument",
            unexpected,
        ),
    });

    result.add_fn("to_string", |vm, args| match vm.get_args(args) {
        [iterable] if iterable.is_iterable() => {
            let iterable = iterable.clone();
            let iterator = vm.make_iterator(iterable)?;
            let (size_hint, _) = iterator.size_hint();
            let mut result = String::with_capacity(size_hint);

            for output in iterator.map(collect_pair) {
                match output {
                    Output::Value(Str(s)) => result.push_str(&s),
                    Output::Value(value) => result.push_str(&value.to_string()),
                    Output::Error(error) => return Err(error),
                    _ => unreachable!(),
                }
            }

            Ok(Str(result.into()))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.to_string",
            "an iterable value as argument",
            unexpected,
        ),
    });

    result.add_fn("to_tuple", |vm, args| match vm.get_args(args) {
        [iterable] if iterable.is_iterable() => {
            let iterable = iterable.clone();
            let iterator = vm.make_iterator(iterable)?;
            let (size_hint, _) = iterator.size_hint();
            let mut result = Vec::with_capacity(size_hint);

            for output in iterator.map(collect_pair) {
                match output {
                    Output::Value(value) => result.push(value),
                    Output::Error(error) => return Err(error),
                    _ => unreachable!(),
                }
            }

            Ok(Tuple(result.into()))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.tuple",
            "an iterable value as argument",
            unexpected,
        ),
    });

    result.add_fn("windows", |vm, args| match vm.get_args(args) {
        [iterable, Number(n)] if iterable.is_sequence() && *n >= 1 => {
            let iterable = iterable.clone();
            let n = *n;
            let result = adaptors::Windows::new(vm.make_iterator(iterable)?, n.into());
            Ok(Iterator(ValueIterator::make_external(result)))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.windows",
            "a value with a range (like a List or String), \
             and a chunk size greater than zero as arguments",
            unexpected,
        ),
    });

    result.add_fn("zip", |vm, args| match vm.get_args(args) {
        [iterable_a, iterable_b] if iterable_a.is_iterable() && iterable_b.is_iterable() => {
            let iterable_a = iterable_a.clone();
            let iterable_b = iterable_b.clone();
            let result =
                adaptors::Zip::new(vm.make_iterator(iterable_a)?, vm.make_iterator(iterable_b)?);
            Ok(Iterator(ValueIterator::make_external(result)))
        }
        unexpected => unexpected_type_error_with_slice(
            "iterator.zip",
            "two iterable values as arguments",
            unexpected,
        ),
    });

    result
}

pub(crate) fn collect_pair(iterator_output: Output) -> Output {
    match iterator_output {
        Output::ValuePair(first, second) => Output::Value(Value::Tuple(vec![first, second].into())),
        _ => iterator_output,
    }
}

fn fold_with_operator(
    vm: &mut Vm,
    iterable: Value,
    initial_value: Value,
    operator: BinaryOp,
) -> RuntimeResult {
    let mut result = initial_value;

    for output in vm.make_iterator(iterable)?.map(collect_pair) {
        match output {
            Output::Value(rhs_value) => {
                result = vm.run_binary_op(operator, result, rhs_value)?;
            }
            Output::Error(error) => return Err(error),
            _ => unreachable!(),
        }
    }

    Ok(result)
}

fn run_iterator_comparison(
    vm: &mut Vm,
    iterable: Value,
    invert_result: InvertResult,
) -> RuntimeResult {
    let mut result: Option<Value> = None;

    for iter_output in vm.make_iterator(iterable)?.map(collect_pair) {
        match iter_output {
            Output::Value(value) => {
                result = Some(match result {
                    Some(result) => {
                        compare_values(vm, result.clone(), value.clone(), invert_result)?
                    }
                    None => value,
                })
            }
            Output::Error(error) => return Err(error),
            _ => unreachable!(),
        }
    }

    Ok(result.unwrap_or_default())
}

fn run_iterator_comparison_by_key(
    vm: &mut Vm,
    iterable: Value,
    key_fn: Value,
    invert_result: InvertResult,
) -> RuntimeResult {
    let mut result_and_key: Option<(Value, Value)> = None;

    for iter_output in vm.make_iterator(iterable)?.map(collect_pair) {
        match iter_output {
            Output::Value(value) => {
                let key = vm.run_function(key_fn.clone(), CallArgs::Single(value.clone()))?;
                let value_and_key = (value, key);

                result_and_key = Some(match result_and_key {
                    Some(result_and_key) => {
                        compare_values_with_key(vm, result_and_key, value_and_key, invert_result)?
                    }
                    None => value_and_key,
                });
            }
            Output::Error(error) => return Err(error),
            _ => unreachable!(),
        }
    }

    Ok(result_and_key.map_or(Value::Empty, |(value, _)| value))
}

// Compares two values using BinaryOp::Less
//
// Returns the lesser of the two values, unless `invert_result` is set to Yes
fn compare_values(vm: &mut Vm, a: Value, b: Value, invert_result: InvertResult) -> RuntimeResult {
    use {InvertResult::*, Value::Bool};

    let comparison_result = vm.run_binary_op(BinaryOp::Less, a.clone(), b.clone())?;

    match (comparison_result, invert_result) {
        (Bool(true), No) => Ok(a),
        (Bool(false), No) => Ok(b),
        (Bool(true), Yes) => Ok(b),
        (Bool(false), Yes) => Ok(a),
        (other, _) => runtime_error!(
            "Expected Bool from '<' comparison, found '{}'",
            other.type_as_string()
        ),
    }
}

// Compares two values using BinaryOp::Less
//
// Returns the lesser of the two values, unless `invert_result` is set to Yes
fn compare_values_with_key(
    vm: &mut Vm,
    a_and_key: (Value, Value),
    b_and_key: (Value, Value),
    invert_result: InvertResult,
) -> Result<(Value, Value), RuntimeError> {
    use {InvertResult::*, Value::Bool};

    let comparison_result =
        vm.run_binary_op(BinaryOp::Less, a_and_key.1.clone(), b_and_key.1.clone())?;

    match (comparison_result, invert_result) {
        (Bool(true), No) => Ok(a_and_key),
        (Bool(false), No) => Ok(b_and_key),
        (Bool(true), Yes) => Ok(b_and_key),
        (Bool(false), Yes) => Ok(a_and_key),
        (other, _) => runtime_error!(
            "Expected Bool from '<' comparison, found '{}'",
            other.type_as_string()
        ),
    }
}

#[derive(Clone, Copy)]
enum InvertResult {
    Yes,
    No,
}
