use crate::{runtime_error, unexpected_type_error_with_slice, Value, ValueMap, ValueTuple};

pub fn make_module() -> ValueMap {
    use Value::*;

    let result = ValueMap::new();

    result.add_value("args", Tuple(ValueTuple::default()));

    result.add_fn("exports", |vm, _| Ok(Value::Map(vm.exports().clone())));

    result.add_fn("run", |vm, args| match vm.get_args(args) {
        [Str(script)] => {
            let chunk = match vm.loader().borrow_mut().compile_script(script, &None) {
                Ok(chunk) => chunk,
                Err(error) => {
                    return runtime_error!("koto.run: error during compilation - {error}")
                }
            };

            match vm.run(chunk) {
                result @ Ok(_) => result,
                Err(error) => runtime_error!("koto.run: runtime error - {error:#}"),
            }
        }
        unexpected => {
            unexpected_type_error_with_slice("koto.run", "a String", unexpected)
        }
    });

    result.add_value("script_dir", Null);
    result.add_value("script_path", Null);

    result.add_fn("type", |vm, args| match vm.get_args(args) {
        [value] => Ok(Str(value.type_as_string().into())),
        unexpected => {
            unexpected_type_error_with_slice("koto.type", "a single argument", unexpected)
        }
    });

    result
}
