# Ownership

Every value in Rust has one owner. When the owner goes out of scope, the value is dropped. Assigning a value moves it unless the type is Copy.

# Borrowing

References borrow a value without taking ownership. You can have many shared references or one mutable reference, never both. The borrow checker enforces this at compile time.

# Lifetimes

Lifetimes name how long a reference is valid. Most are inferred; function signatures sometimes need explicit lifetime parameters such as `'a`.
