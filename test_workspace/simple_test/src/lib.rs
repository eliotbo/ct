//! Test module for ct-indexer

/// A simple struct
#[derive(Clone, Debug)]
pub struct TestStruct {
    pub field1: String,
    field2: i32,
}

impl TestStruct {
    /// Creates a new TestStruct
    pub fn new(field1: String, field2: i32) -> Self {
        Self { field1, field2 }
    }

    /// A method that's not implemented yet
    pub fn unimplemented_method(&self) {
        unimplemented!("This method is not implemented")
    }

    /// A method with TODO
    pub fn todo_method(&self) -> i32 {
        // TODO: implement this properly
        todo!("Need to implement this")
    }

    /// Clone method from derive - should be filtered out
    pub fn clone(&self) -> Self {
        Self {
            field1: self.field1.clone(),
            field2: self.field2,
        }
    }
}

/// A trait
pub trait TestTrait {
    fn trait_method(&self) -> String;
}

impl TestTrait for TestStruct {
    fn trait_method(&self) -> String {
        format!("TestStruct: {}", self.field1)
    }
}

/// An enum
pub enum TestEnum {
    Variant1,
    Variant2(String),
    Variant3 { field: i32 },
}

/// A module
pub mod submodule {
    /// A function in a submodule
    pub fn submodule_function() {
        println!("In submodule");
    }
}

/// A type alias
pub type TestAlias = Vec<TestStruct>;

/// A constant
pub const TEST_CONST: i32 = 42;

/// A static
pub static TEST_STATIC: &str = "test";