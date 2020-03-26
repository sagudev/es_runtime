use log::trace;

use std::cell::RefCell;

use mozjs::jsapi::JSContext;

use mozjs::jsapi::JSObject;

use mozjs::rust::HandleValue;

use crate::es_utils;
use mozjs::jsval::{BooleanValue, DoubleValue, Int32Value, ObjectValue, UndefinedValue};
use std::collections::HashMap;

use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

/// the EsValueFacade is a converter between rust variables and script objects
/// when receiving a EsValueFacade from the script engine it's data is allways a clone from the actual data so we need not worry about the value being garbage collected
///

struct RustManagedEsVar {
    obj_id: i32,
    opt_receiver: Option<Receiver<Result<EsValueFacade, EsValueFacade>>>,
}

pub struct EsValueFacade {
    val_string: Option<String>,
    val_i32: Option<i32>,
    val_f64: Option<f64>,
    val_boolean: Option<bool>,
    val_managed_var: Option<RustManagedEsVar>,

    val_object: Option<HashMap<String, EsValueFacade>>,
}

thread_local! {
    static PROMISE_RESOLUTION_TRANSMITTERS: RefCell<HashMap<i32, Sender<Result<EsValueFacade, EsValueFacade>>>> =
        { RefCell::new(HashMap::new()) };
}

impl EsValueFacade {
    pub(crate) fn resolve_future(man_obj_id: i32, res: Result<EsValueFacade, EsValueFacade>) -> () {
        PROMISE_RESOLUTION_TRANSMITTERS.with(|rc| {
            let map: &mut HashMap<i32, Sender<Result<EsValueFacade, EsValueFacade>>> =
                &mut *rc.borrow_mut();
            let opt: Option<Sender<Result<EsValueFacade, EsValueFacade>>> = map.remove(&man_obj_id);
            if opt.is_some() {
                opt.unwrap().send(res).expect("could not send res");
            } else {
                panic!("no transmitter found {}", man_obj_id);
            }
        })
    }

    pub fn undefined() -> Self {
        // todo cache
        EsValueFacade {
            val_string: None,
            val_f64: None,
            val_i32: None,
            val_boolean: None,
            val_managed_var: None,
            val_object: None,
        }
    }

    pub fn new_f64(num: f64) -> Self {
        EsValueFacade {
            val_string: None,
            val_f64: Some(num),
            val_i32: None,
            val_boolean: None,
            val_managed_var: None,
            val_object: None,
        }
    }

    pub fn new_obj(props: HashMap<String, EsValueFacade>) -> Self {
        EsValueFacade {
            val_string: None,
            val_f64: None,
            val_i32: None,
            val_boolean: None,
            val_managed_var: None,
            val_object: Some(props),
        }
    }

    pub fn new_i32(num: i32) -> Self {
        EsValueFacade {
            val_string: None,
            val_i32: Some(num),
            val_f64: None,
            val_boolean: None,
            val_managed_var: None,
            val_object: None,
        }
    }

    pub fn new_str(s: String) -> Self {
        EsValueFacade {
            val_string: Some(s),
            val_i32: None,
            val_f64: None,
            val_boolean: None,
            val_managed_var: None,
            val_object: None,
        }
    }

    pub fn new_bool(b: bool) -> Self {
        EsValueFacade {
            val_string: None,
            val_i32: None,
            val_f64: None,
            val_boolean: Some(b),
            val_managed_var: None,
            val_object: None,
        }
    }

    pub fn new(context: *mut JSContext, rval: HandleValue) -> Self {
        Self::new_v(context, *rval)
    }

    pub fn new_v(context: *mut JSContext, rval: mozjs::jsapi::Value) -> Self {
        let mut val_string = None;
        let mut val_i32 = None;
        let mut val_f64 = None;
        let mut val_boolean = None;
        let mut val_managed_var = None;
        let mut val_object = None;

        if rval.is_boolean() {
            val_boolean = Some(rval.to_boolean());
        } else if rval.is_int32() {
            val_i32 = Some(rval.to_int32());
        } else if rval.is_double() {
            val_f64 = Some(rval.to_number());
        } else if rval.is_string() {
            let es_str = es_utils::es_value_to_str(context, &rval);

            trace!("EsValueFacade::new got string {}", es_str);

            val_string = Some(es_str);
        } else if rval.is_object() {
            let mut map = HashMap::new();
            let obj: *mut JSObject = rval.to_object();
            rooted!(in(context) let mut _obj_root = obj);

            let prop_names: Vec<String> = crate::es_utils::get_js_obj_prop_names(context, obj);

            if prop_names.contains(&"__esses_future_obj_id".to_string()) {
                let obj_id_val =
                    crate::es_utils::get_es_obj_prop_val(context, obj, "__esses_future_obj_id");

                let obj_id = obj_id_val.to_int32();

                let (tx, rx) = channel();
                let opt_receiver = Some(rx);

                PROMISE_RESOLUTION_TRANSMITTERS.with(move |rc| {
                    let map: &mut HashMap<i32, Sender<Result<EsValueFacade, EsValueFacade>>> =
                        &mut *rc.borrow_mut();
                    map.insert(obj_id, tx);
                });

                let rmev: RustManagedEsVar = RustManagedEsVar {
                    obj_id: obj_id_val.to_int32(),
                    opt_receiver,
                };

                val_managed_var = Some(rmev);
            } else {
                for prop_name in prop_names {
                    let prop_val: mozjs::jsapi::Value =
                        crate::es_utils::get_es_obj_prop_val(context, obj, prop_name.as_str());
                    let prop_esvf = EsValueFacade::new_v(context, prop_val);
                    map.insert(prop_name, prop_esvf);
                }
            }

            val_object = Some(map);
        }

        let ret = EsValueFacade {
            val_string,
            val_i32,
            val_f64,
            val_boolean,
            val_managed_var,
            val_object,
        };

        ret
    }

    pub fn get_string(&self) -> &String {
        self.val_string.as_ref().expect("not a string")
    }
    pub fn get_i32(&self) -> &i32 {
        &self.val_i32.as_ref().expect("i am not a i32")
    }
    pub fn get_f64(&self) -> &f64 {
        &self.val_f64.as_ref().expect("i am not a f64")
    }
    pub fn get_boolean(&self) -> bool {
        self.val_boolean.expect("i am not a boolean")
    }
    pub fn get_managed_object_id(&self) -> i32 {
        let rmev: &RustManagedEsVar = self.val_managed_var.as_ref().expect("not a managed var");
        rmev.obj_id.clone()
    }

    pub fn is_promise(&self) -> bool {
        self.is_managed_object()
    }

    pub fn get_promise_result_blocking(
        &self,
        timeout: Duration,
    ) -> Result<Result<EsValueFacade, EsValueFacade>, RecvTimeoutError> {
        // ok, hier gaan we dus pas .then en .catch aan de promise hangen
        // hier gooien we ook pas de sender in een thread_local via een job
        // dus de sender leeft in de worker thread thread_local

        if !self.is_promise() {
            return Ok(Err(EsValueFacade::new_str(
                "esvf was not a Promise".to_string(),
            )));
        }

        let rmev: &RustManagedEsVar = self.val_managed_var.as_ref().expect("not a managed var");
        let rx = rmev.opt_receiver.as_ref().expect("not a waiting promise");

        let rx_result = rx.recv_timeout(timeout);

        return rx_result;
    }

    pub fn get_object(&self) -> &HashMap<String, EsValueFacade> {
        return self.val_object.as_ref().unwrap();
    }

    pub fn is_string(&self) -> bool {
        self.val_string.is_some()
    }
    pub fn is_i32(&self) -> bool {
        self.val_i32.is_some()
    }
    pub fn is_f64(&self) -> bool {
        self.val_f64.is_some()
    }
    pub fn is_boolean(&self) -> bool {
        self.val_boolean.is_some()
    }
    pub fn is_managed_object(&self) -> bool {
        self.val_managed_var.is_some()
    }
    pub fn is_object(&self) -> bool {
        self.val_object.is_some()
    }

    pub fn as_js_expression_str(&self) -> String {
        if self.is_boolean() {
            if self.get_boolean() {
                return "true".to_string();
            } else {
                return "false".to_string();
            }
        } else if self.is_i32() {
            return format!("{}", self.get_i32());
        } else if self.is_f64() {
            return format!("{}", self.get_f64());
        } else if self.is_string() {
            return format!("\"{}\"", self.get_string());
        } else if self.is_managed_object() {
            return format!("/* Future {} */", self.get_managed_object_id());
        } else if self.is_object() {
            let mut res: String = String::new();
            let map = self.get_object();
            res.push('{');
            for e in map {
                if res.len() > 1 {
                    res.push_str(", ");
                }
                res.push('"');
                res.push_str(e.0);
                res.push_str("\": ");

                res.push_str(e.1.as_js_expression_str().as_str());
            }

            res.push('}');
            return res;
        }
        "null".to_string()
    }

    pub(crate) fn to_es_value(&self, context: *mut JSContext) -> mozjs::jsapi::Value {
        trace!("to_es_value.1");

        if self.is_i32() {
            trace!("to_es_value.2");
            return Int32Value(self.get_i32().clone());
        } else if self.is_f64() {
            trace!("to_es_value.3");
            return DoubleValue(self.get_f64().clone());
        } else if self.is_boolean() {
            trace!("to_es_value.4");
            return BooleanValue(self.get_boolean());
        } else if self.is_string() {
            trace!("to_es_value.5");
            return es_utils::new_es_value_from_str(context, self.get_string());
        } else if self.is_object() {
            trace!("to_es_value.6");
            let obj: *mut JSObject = es_utils::new_object(context);
            rooted!(in(context) let mut obj_root = obj);
            let map = self.get_object();
            for prop in map {
                let prop_name = prop.0;
                let prop_esvf = prop.1;
                let prop_val: mozjs::jsapi::Value = prop_esvf.to_es_value(context);
                rooted!(in(context) let mut val_root = prop_val);
                es_utils::set_es_obj_prop_val(
                    context,
                    obj_root.handle(),
                    prop_name,
                    val_root.handle(),
                );
            }

            return ObjectValue(obj);
        } else {
            // todo, other val types
            trace!("to_es_value.7");
            return UndefinedValue();
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::esvaluefacade::EsValueFacade;
    use crate::spidermonkeyruntimewrapper::SmRuntime;
    use std::collections::HashMap;
    use std::time::Duration;

    #[test]
    fn in_and_output_vars() {
        println!("in_and_output_vars_1");

        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        rt.do_with_inner(|inner| {
            inner.register_op(
                "test_op_0",
                Box::new(|_sm_rt: &SmRuntime, args: Vec<EsValueFacade>| {
                    let args1 = args.get(0).expect("did not get a first arg");
                    let args2 = args.get(1).expect("did not get a second arg");

                    let x = args1.get_i32().clone() as f64;
                    let y = args2.get_i32().clone() as f64;

                    return Ok(EsValueFacade::new_f64(x / y));
                }),
            );
            inner.register_op(
                "test_op_1",
                Box::new(|_sm_rt: &SmRuntime, args: Vec<EsValueFacade>| {
                    let args1 = args.get(0).expect("did not get a first arg");
                    let args2 = args.get(1).expect("did not get a second arg");

                    let x = args1.get_i32();
                    let y = args2.get_i32();

                    return Ok(EsValueFacade::new_i32(x * y));
                }),
            );

            inner.register_op(
                "test_op_2",
                Box::new(|_sm_rt: &SmRuntime, args: Vec<EsValueFacade>| {
                    let args1 = args.get(0).expect("did not get a first arg");
                    let args2 = args.get(1).expect("did not get a second arg");

                    let x = args1.get_i32();
                    let y = args2.get_i32();

                    return Ok(EsValueFacade::new_bool(x > y));
                }),
            );

            inner.register_op(
                "test_op_3",
                Box::new(|_sm_rt: &SmRuntime, args: Vec<EsValueFacade>| {
                    let args1 = args.get(0).expect("did not get a first arg");
                    let args2 = args.get(1).expect("did not get a second arg");

                    let x = args1.get_i32();
                    let y = args2.get_i32();

                    let res_str = format!("{}", x * y);
                    return Ok(EsValueFacade::new_str(res_str));
                }),
            );

            let res0 = inner.eval_sync(
                "return esses.invoke_rust_op_sync('test_op_0', 13, 17);",
                "test_vars0.es",
            );
            let res1 = inner.eval_sync(
                "return esses.invoke_rust_op_sync('test_op_1', 13, 17);",
                "test_vars1.es",
            );
            let res2 = inner.eval_sync(
                "return esses.invoke_rust_op_sync('test_op_2', 13, 17);",
                "test_vars2.es",
            );
            let res3 = inner.eval_sync(
                "return esses.invoke_rust_op_sync('test_op_3', 13, 17);",
                "test_vars3.es",
            );
            let esvf0 = res0.ok().expect("1 did not get a result");
            let esvf1 = res1.ok().expect("1 did not get a result");
            let esvf2 = res2.ok().expect("2 did not get a result");
            let esvf3 = res3.ok().expect("3 did not get a result");

            assert_eq!(esvf0.get_f64().clone(), (13 as f64 / 17 as f64));
            assert_eq!(esvf1.get_i32().clone(), (13 * 17) as i32);
            assert_eq!(esvf2.get_boolean(), false);
            assert_eq!(esvf3.get_string(), format!("{}", 13 * 17).as_str());
        });
    }

    #[test]
    fn test_wait_for_native_prom() {
        println!("test_wait_for_native_prom");

        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        let esvf_prom = rt
            .eval_sync(
                "let p = new Promise((resolve, reject) => {resolve(123);});p = p.then((v) => {return v;});p = p.then((v) => {return v;});p = p.then((v) => {return v;});p = p.then((v) => {return v;});p = p.then((v) => {return v;});p = p.then((v) => {return v;});return p;",
                "wait_for_prom.es",
            )
            .ok()
            .unwrap();
        assert!(esvf_prom.is_promise());
        let esvf_prom_resolved = esvf_prom
            .get_promise_result_blocking(Duration::from_secs(60))
            .ok()
            .unwrap()
            .ok()
            .unwrap();

        assert!(esvf_prom_resolved.is_i32());
        assert_eq!(esvf_prom_resolved.get_i32().clone(), 123 as i32);
    }

    #[test]
    fn test_wait_for_prom() {
        println!("test_wait_for_prom_1");

        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        let esvf_prom = rt
            .eval_sync(
                "let p = new Promise((resolve, reject) => {resolve(123);});return p;",
                "wait_for_prom.es",
            )
            .ok()
            .unwrap();
        assert!(esvf_prom.is_promise());
        let esvf_prom_resolved = esvf_prom
            .get_promise_result_blocking(Duration::from_secs(60))
            .ok()
            .unwrap()
            .ok()
            .unwrap();

        assert!(esvf_prom_resolved.is_i32());
        assert_eq!(esvf_prom_resolved.get_i32().clone(), 123 as i32);
    }

    #[test]
    fn test_wait_for_prom2() {
        println!("test_wait_for_prom_2");

        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        let esvf_prom = rt
            .eval_sync(
                "let p = new Promise((resolve, reject) => {reject(\"foo\");});return p;",
                "wait_for_prom.es",
            )
            .ok()
            .unwrap();
        assert!(esvf_prom.is_promise());
        let esvf_prom_resolved = esvf_prom
            .get_promise_result_blocking(Duration::from_secs(60))
            .ok()
            .unwrap()
            .err()
            .unwrap();

        assert!(esvf_prom_resolved.is_string());

        assert_eq!(esvf_prom_resolved.get_string(), "foo");
    }

    #[test]
    fn test_get_object() {
        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        let esvf = rt
            .eval_sync(
                "return {a: 1, b: true, c: 'hello', d: {a: 2}};",
                "test_get_object.es",
            )
            .ok()
            .unwrap();

        assert!(esvf.is_object());

        let map: &HashMap<String, EsValueFacade> = esvf.get_object();

        let esvf_a = map.get(&"a".to_string()).unwrap();

        assert!(esvf_a.is_i32());
        assert_eq!(esvf_a.get_i32(), &1);
    }

    #[test]
    fn test_set_object() {
        let rt = crate::esruntimewrapper::tests::TEST_RT.clone();
        let _esvf = rt
            .eval_sync(
                "this.test_set_object = function test_set_object(obj, prop){return obj[prop];};",
                "test_set_object_1.es",
            )
            .ok()
            .unwrap();

        let mut map: HashMap<String, EsValueFacade> = HashMap::new();
        map.insert(
            "p1".to_string(),
            EsValueFacade::new_str("hello".to_string()),
        );
        let obj = EsValueFacade::new_obj(map);

        let res_esvf_res = rt.call_sync(
            "test_set_object",
            vec![obj, EsValueFacade::new_str("p1".to_string())],
        );

        let res_esvf = res_esvf_res.ok().unwrap();
        assert!(res_esvf.is_string());
        assert_eq!(res_esvf.get_string(), "hello");
    }
}