use {crate::Poetry, koto::prelude::*};

pub fn make_module() -> ValueMap {
    let result = ValueMap::new();

    result.add_fn("new", {
        |vm, args| match vm.get_args(args) {
            [Value::Str(text)] => {
                let mut poetry = Poetry::default();
                poetry.add_source_material(text);
                Ok(KotoPoetry::make_external_value(poetry))
            }
            unexpected => type_error_with_slice("a String", unexpected),
        }
    });

    result
}

thread_local! {
    static POETRY_BINDINGS: PtrMut<MetaMap> = make_poetry_meta_map();
}

fn make_poetry_meta_map() -> PtrMut<MetaMap> {
    use Value::{Null, Str};

    MetaMapBuilder::<KotoPoetry>::new("Poetry")
        .function("add_source_material", |context| match context.args {
            [Str(text)] => {
                context.data_mut()?.0.add_source_material(text);
                Ok(Null)
            }
            unexpected => type_error_with_slice("a String", unexpected),
        })
        .function("iter", |context| {
            let iter = PoetryIter {
                poetry: context.external.clone(),
            };
            Ok(ValueIterator::new(iter).into())
        })
        .function("next_word", |context| {
            let result = match context.data_mut()?.0.next_word() {
                Some(word) => Str(word.as_ref().into()),
                None => Null,
            };
            Ok(result)
        })
        .build()
}

#[derive(Clone)]
struct PoetryIter {
    poetry: External,
}

impl KotoIterator for PoetryIter {
    fn make_copy(&self) -> ValueIterator {
        ValueIterator::new(self.clone())
    }
}

impl Iterator for PoetryIter {
    type Item = ValueIteratorOutput;

    fn next(&mut self) -> Option<Self::Item> {
        use Value::{Null, Str};

        match self.poetry.data_mut::<KotoPoetry>() {
            Some(mut poetry) => {
                let result = match poetry.0.next_word() {
                    Some(word) => Str(word.as_ref().into()),
                    None => Null,
                };
                Some(ValueIteratorOutput::Value(result))
            }
            None => Some(ValueIteratorOutput::Error(make_runtime_error!(
                "Unexpected internal data type"
            ))),
        }
    }
}

#[derive(Clone, Debug)]
pub struct KotoPoetry(Poetry);

impl KotoPoetry {
    fn make_external_value(poetry: Poetry) -> Value {
        let result = External::with_shared_meta_map(
            KotoPoetry(poetry),
            POETRY_BINDINGS.with(|meta| meta.clone()),
        );

        Value::External(result)
    }
}

impl ExternalData for KotoPoetry {
    fn make_copy(&self) -> PtrMut<dyn ExternalData> {
        make_data_ptr(self.clone())
    }
}
