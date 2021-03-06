extern crate mttester;

use mttester::MtManager;
use mttester::MtManagerTrait;
use std::sync::{Arc, Mutex};
use mttester::MtModifierTrait;

#[derive(Clone, Default)]
struct Custom;

// impl Clone for Custom {
//     fn clone(&self) -> Self {
//         let c = Custom;
//         c
//     }
// }

// impl Default for Custom {
//     fn default() -> Self {
//         let c = Custom;
//         c
//     }
// }


impl MtModifierTrait for Custom {
    fn trans(&self, index: i64) -> String {
        index.to_string()
    }
}


fn main() {

    let incr_index = Arc::new(Mutex::new(10));
    
    
    let mut manager: MtManager<Custom> = MtManager::new();
    let c = Custom;
    
    manager.set_url("http://www.artselleasy.com/ysxy/api/art/1789".to_string(), "GET".to_string(), "urlencoded".to_string())
        .add_param("foo".to_string(), "bar".to_string())
        .add_closure_param("test".to_string(), Box::new(move || "hello".to_string()) )
        .add_closure_param("pageid".to_string(), Box::new(move || {
            let mut incr = incr_index.lock().unwrap();
            *incr += 1;
            //println!("{}", *incr);
            (*incr).to_string()
        }))
        .add_modifier_param("life".to_string(), c)
        .set_seconds(5)
        .set_threads(5)
        .start();
    
    println!("End.");
}

