//! Deref + 方法覆盖的示例

use std::ops::Deref;

// ============================================================
// 父类
// ============================================================

pub struct Parent {
    pub name: String,
}

impl Parent {
    pub fn say_hello(&self) {
        println!("[Parent] hello: {}", self.name);
    }

    pub fn say_bye(&self) {
        println!("[Parent] bye: {}", self.name);
    }
}

// ============================================================
// 子类 (使用 Deref)
// ============================================================

pub struct Child {
    pub base: Parent,
    pub age: u32,
}

impl Deref for Child {
    type Target = Parent;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl Child {
    // ⭐ 覆盖父类的 say_hello
    pub fn say_hello(&self) {
        println!("[Child] hello: {}, age: {}", self.name, self.age);
    }

    // say_bye 不覆盖，自动通过 Deref 访问 Parent 的实现

    pub fn say_age(&self) {
        println!("[Child] I'm {} years old", self.age);
    }
}

// ============================================================
// 孙类
// ============================================================

pub struct GrandChild {
    pub base: Child,
    pub grade: u32,
}

impl Deref for GrandChild {
    type Target = Child;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl GrandChild {
    // ⭐ 再次覆盖 say_hello
    pub fn say_hello(&self) {
        println!("[GrandChild] hello: {}, age: {}, grade: {}",
            self.name, self.age, self.grade);
    }

    // say_bye 不覆盖，通过 Deref 链访问 Parent 的实现
    // say_age 不覆盖，通过 Deref 访问 Child 的实现
}

// ============================================================
// 演示
// ============================================================

fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  Deref + 方法覆盖                                              ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    let parent = Parent { name: "parent".to_string() };
    let child = Child { base: Parent { name: "child".to_string() }, age: 10 };
    let grandchild = GrandChild {
        base: Child { base: Parent { name: "grandchild".to_string() }, age: 15 },
        grade: 5,
    };

    // ── 1. say_hello：每层都覆盖了 ──
    println!("1. say_hello (每层都覆盖):");
    print!("   Parent:    "); parent.say_hello();
    print!("   Child:     "); child.say_hello();      // 调用 Child 的覆盖版本
    print!("   GrandChild:"); grandchild.say_hello(); // 调用 GrandChild 的覆盖版本
    println!();

    // ── 2. say_bye：没有覆盖，通过 Deref 访问 Parent ──
    println!("2. say_bye (只有 Parent 有，其他通过 Deref 访问):");
    print!("   Parent:    "); parent.say_bye();
    print!("   Child:     "); child.say_bye();        // Deref → Parent.say_bye
    print!("   GrandChild:"); grandchild.say_bye();   // Deref → Deref → Parent.say_bye
    println!();

    // ── 3. 明确调用特定实现 ──
    println!("3. 明确调用特定实现:");
    print!("   Child 调用 Parent.say_hello:    ");
    Parent::say_hello(&child.base);  // 或者 <Parent as _>::say_hello(&child)

    print!("   GrandChild 调用 Child.say_hello: ");
    Child::say_hello(&grandchild.base);

    print!("   GrandChild 调用 Parent.say_hello: ");
    Parent::say_hello(&grandchild.base.base);
    println!();

    // ── 4. 访问字段 ──
    println!("4. 访问字段 (通过 Deref 链):");
    println!("   grandchild.name = '{}'  (Parent 的字段)", grandchild.name);
    println!("   grandchild.age = {}    (Child 的字段)", grandchild.age);
    println!("   grandchild.grade = {}  (GrandChild 的字段)", grandchild.grade);
    println!();

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  总结：                                                        ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  1. Deref 不阻止覆盖方法                                       ║");
    println!("║  2. 子类同名方法会遮蔽父类方法                            ║");
    println!("║  3. 可以通过 Parent::method(&child.base) 调用父类版本          ║");
    println!("║  4. 未覆盖的方法自动通过 Deref 访问父类                        ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
}
