use std::panic::{AssertUnwindSafe, catch_unwind};

/// worker 边界：坏包触发的 panic 必须转成任务失败，不能带崩进程。
///
/// 注意：`[profile.release]` 不能设 `panic = "abort"`，否则本函数失效。
pub fn guarded<R>(f: impl FnOnce() -> R) -> Result<R, String> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|payload| panic_message(&payload))
}

fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "worker panic（未知载荷）".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::guarded;

    #[test]
    fn converts_panic_into_error() {
        let result = guarded(|| panic!("boom"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("boom"));
    }

    #[test]
    fn passes_value_through() {
        assert_eq!(guarded(|| 42).unwrap(), 42);
    }
}
