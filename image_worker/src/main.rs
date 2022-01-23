#[cfg(target_arch = "wasm32")]
mod op {
    use js_sys::Uint8Array;
    use wasm_bindgen::{prelude::*, JsCast};
    use web_sys::{DedicatedWorkerGlobalScope, MessageEvent};

    use image_worker::load_image;

    pub fn main() {
        let worker_scope: DedicatedWorkerGlobalScope =
            js_sys::eval("self").expect_throw("cant get self").unchecked_into();

        let handler = {
            let worker_scope = worker_scope.clone();

            Closure::wrap(Box::new(move |event: MessageEvent| {
                let data: Uint8Array = event.data().unchecked_into();
                let data: Vec<u8> = load_image(data.to_vec());
                let result = Uint8Array::new_with_length(data.len() as u32);
                unsafe {
                    result.set(&Uint8Array::view(&data), 0);
                }

                worker_scope.post_message(&result).expect_throw("can't send message");
            }) as Box<dyn FnMut(_)>)
        };

        worker_scope.set_onmessage(Some(handler.as_ref().unchecked_ref()));

        handler.forget();
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod op {
    pub fn main() {}
}

fn main() {
    op::main();
}
