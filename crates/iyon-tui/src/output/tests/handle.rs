use crate::output::Output;

struct NonClone {
    text: String,
}

fn assert_copy<T: Copy>() {}
fn assert_clone<T: Clone>() {}
fn assert_eq<T: Eq>() {}
fn assert_hash<T: std::hash::Hash>() {}
fn assert_debug<T: std::fmt::Debug>() {}

#[test]
fn handles_have_opaque_unique_identity_and_copy_preserves_it() {
    let first = Output::<usize>::new();
    let second = Output::<usize>::new();
    let copy = first;

    assert_ne!(first, second);
    assert_eq!(first, copy);
    assert_eq!(format!("{first:?}"), "Output");
}

#[test]
fn handle_traits_do_not_require_payload_traits() {
    assert_copy::<Output<NonClone>>();
    assert_clone::<Output<NonClone>>();
    assert_eq::<Output<NonClone>>();
    assert_hash::<Output<NonClone>>();
    assert_debug::<Output<NonClone>>();
}

#[test]
fn handles_with_the_same_type_are_stable_but_instances_are_distinct() {
    struct Emitter {
        changed: Output<usize>,
    }

    impl Emitter {
        fn changed(&self) -> Output<usize> {
            self.changed
        }
    }

    let first = Emitter {
        changed: Output::new(),
    };
    let second = Emitter {
        changed: Output::new(),
    };

    assert_eq!(first.changed(), first.changed());
    assert_ne!(first.changed(), second.changed());
}

#[test]
fn output_handles_do_not_require_payload_send_or_clone() {
    let output = Output::<NonClone>::new();
    let _ = output;

    let local = std::rc::Rc::new(String::from("local"));
    let output = Output::<std::rc::Rc<String>>::new();
    let _ = (output, local);
}
