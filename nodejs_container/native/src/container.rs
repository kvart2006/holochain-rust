use holochain_cas_implementations::{
    cas::{file::FilesystemStorage, memory::MemoryStorage},
    eav::memory::EavMemoryStorage,
};
use holochain_container_api::{
    config::{
        AgentConfiguration, Configuration, DNAConfiguration, InstanceConfiguration,
        LoggerConfiguration, StorageConfiguration,
    },
    container::Container,
    Holochain,
};
use holochain_core::{
    context::{mock_network_config, Context as HolochainContext},
    logger::Logger,
    persister::SimplePersister,
    signal::{signal_channel, Signal, SignalReceiver},
};
use holochain_core_types::{agent::AgentId, dna::Dna, json::JsonString};
use neon::{context::Context, prelude::*};
use std::{
    convert::TryFrom,
    path::PathBuf,
    sync::{Arc, Mutex, RwLock},
    thread,
};
use tempfile::tempdir;

use crate::config::*;

#[derive(Clone, Debug)]
struct NullLogger {}

impl Logger for NullLogger {
    fn log(&mut self, _msg: String) {}
}

pub struct Habitat {
    container: Container,
    // signal_task: HabitatSignalDispatcher,
}

fn signal_callback(mut cx: FunctionContext) -> JsResult<JsNull> {
    panic!("never should happen");
    Ok(cx.null())
}

declare_types! {

    pub class JsHabitat for Habitat {
        init(mut cx) {
            let config_arg = cx.argument(0)?;
            let config = neon_serde::from_value(&mut cx, config_arg)?;
            let container = Container::from_config(config);
            Ok(Habitat { container })
        }

        method start(mut cx) {
            let js_callback = JsFunction::new(&mut cx, signal_callback).unwrap();
            let mut this = cx.this();

            let start_result: Result<(), String> = {
                let guard = cx.lock();
                let hab = &mut *this.borrow_mut(&guard);
                let (signal_tx, signal_rx) = signal_channel();
                hab.container.load_config_with_signal(Some(signal_tx)).and_then(|_| {
                    let signal_task = HabitatSignalDispatcher {signal_rx};
                    signal_task.schedule(js_callback);
                    hab.container.start_all_instances().map_err(|e| e.to_string())
                })
            };

            start_result.or_else(|e| {
                let error_string = cx.string(format!("unable to start habitat: {}", e));
                cx.throw(error_string)
            })?;

            Ok(cx.undefined().upcast())
        }

        // method start_signal_system(mut cx) {
        //     let mut this = cx.this();

        //     let guard = cx.lock();
        //     let hab = &mut *this.borrow_mut(&guard);
        //     hab.signal_task.schedule(JsFunction::new(&mut cx, signal_callback).unwrap());
        //     Ok(cx.undefined().upcast())
        // }

        method stop(mut cx) {
            let mut this = cx.this();

            let stop_result: Result<(), String> = {
                let guard = cx.lock();
                let hab = &mut *this.borrow_mut(&guard);
                hab.container.stop_all_instances().map_err(|e| e.to_string())
            };

            stop_result.or_else(|e| {
                let error_string = cx.string(format!("unable to stop habitat: {}", e));
                cx.throw(error_string)
            })?;

            Ok(cx.undefined().upcast())
        }

        method call(mut cx) {
            let instance_id = cx.argument::<JsString>(0)?.to_string(&mut cx)?.value();
            let zome = cx.argument::<JsString>(1)?.to_string(&mut cx)?.value();
            let cap = cx.argument::<JsString>(2)?.to_string(&mut cx)?.value();
            let fn_name = cx.argument::<JsString>(3)?.to_string(&mut cx)?.value();
            let params = cx.argument::<JsString>(4)?.to_string(&mut cx)?.value();
            let mut this = cx.this();

            let call_result = {
                let guard = cx.lock();
                let hab = &mut *this.borrow_mut(&guard);
                let instance_arc = hab.container.instances().get(&instance_id)
                    .expect(&format!("No instance with id: {}", instance_id));
                let mut instance = instance_arc.write().unwrap();
                instance.call(&zome, &cap, &fn_name, &params)
            };

            let res_string = call_result.or_else(|e| {
                let error_string = cx.string(format!("unable to call zome function: {:?}", &e));
                cx.throw(error_string)
            })?;

            let result_string: String = res_string.into();
            Ok(cx.string(result_string).upcast())
        }
    }
}

struct HabitatSignalDispatcher {
    signal_rx: SignalReceiver
}

impl HabitatSignalDispatcher {
    pub fn new(signal_rx: SignalReceiver) -> Self {
        let this = Self { signal_rx };
        this
    }
}

impl Task for HabitatSignalDispatcher {
    type Output = ();
    type Error = String;
    type JsEvent = JsNumber;

    fn perform(&self) -> Result<(), String> {
        use std::io::{self, Write};
        while let Ok(sig) = self.signal_rx.recv() {
            print!(".");
            io::stdout().flush().unwrap();
        }
        Ok(())
    }

    fn complete(self, mut cx: TaskContext, result: Result<(), String>) -> JsResult<JsNumber> {
        Ok(cx.number(17))
    }
}

register_module!(mut cx, {
    cx.export_class::<JsHabitat>("Habitat")?;
    cx.export_class::<JsConfigBuilder>("ConfigBuilder")?;
    Ok(())
});
