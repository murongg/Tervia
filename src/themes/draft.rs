use super::*;
use std::collections::BTreeSet;

pub struct Draft {
    original: Definition,
    data: Definition,
    name: String,
    invalid: BTreeSet<String>,
}

impl Draft {
    pub fn new(data: Definition) -> Self {
        Self {
            original: data.clone(),
            name: data.name.clone(),
            data,
            invalid: BTreeSet::new(),
        }
    }
    pub fn data(&self) -> &Definition {
        &self.data
    }
    pub fn can_save(&self) -> bool {
        self.invalid.is_empty()
            && self.data.name == self.name.trim()
            && !self.name.trim().is_empty()
    }
    pub fn is_dirty(&self) -> bool {
        self.data != self.original || self.name != self.original.name || !self.invalid.is_empty()
    }
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
        let _ = self.data.set_name(self.name.clone());
    }
    pub fn set_color(&mut self, key: &str, value: &str) -> Result<(), String> {
        match Color::parse(value).and_then(|color| self.data.set_color(key, color)) {
            Ok(()) => {
                self.invalid.remove(key);
                Ok(())
            }
            Err(error) => {
                self.invalid.insert(key.into());
                Err(error)
            }
        }
    }
    pub fn invalid(&self, key: &str) -> bool {
        self.invalid.contains(key)
    }
    pub fn reset_color(&mut self, key: &str) -> Result<(), String> {
        if let Some(capture) = key.strip_prefix("syntax.") {
            match self.original.syntax.get(capture) {
                Some(style) => self.data.syntax[capture] = style.clone(),
                None => {
                    self.data
                        .syntax
                        .as_object_mut()
                        .expect("Validated syntax map")
                        .remove(capture);
                }
            }
        } else {
            self.data.set_color(key, self.original.color(key)?)?;
        }
        self.invalid.remove(key);
        Ok(())
    }
}
