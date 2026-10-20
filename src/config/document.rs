use toml_edit::{Item, Value};

pub(crate) fn set(item: &mut Item, mut value: Value) {
    if let Some(previous) = item.as_value() {
        if previous
            .as_str()
            .zip(value.as_str())
            .is_some_and(|(a, b)| a == b)
            || previous
                .as_bool()
                .zip(value.as_bool())
                .is_some_and(|(a, b)| a == b)
            || previous
                .as_integer()
                .zip(value.as_integer())
                .is_some_and(|(a, b)| a == b)
        {
            return;
        }
        *value.decor_mut() = previous.decor().clone();
    }
    *item = Item::Value(value);
}
