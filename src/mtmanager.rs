
use std::collections::HashMap;
use std::path::Path;
use std::thread;
use std::sync::mpsc::channel;
use std::sync::mpsc::{Sender, Receiver};
use hyper;
use hyper::Client;
use hyper::client::Response;
use hyper::status::StatusCode;
use hyper::header::Headers;
use hyper::header::ContentType;
use time;
use std::io::Read;
use rustc_serialize::json;
use rustc_serialize::json::Json;
use jsonway;
use url as url_m;
use std::sync::{Arc, Mutex};



pub trait MtModifierTrait: Sized + Clone + Send + Sync {
    fn before(&self, index: i64) -> String;
    fn after(&self, index: i64, res: &String) -> String {
        "".to_string()
    }
}

#[derive(Clone, Default)]
pub struct MtModifier;

impl MtModifierTrait for MtModifier {
    fn before(&self, index: i64) -> String {
        index.to_string()
    }
    
}


#[derive(Default)]
pub struct MtManager<T: MtModifierTrait + Default + 'static> {
    // time to test, unit is second
    time_seconds: i64,
    // test (url, method)
    url: (String, String, String),
    // login, auth (url, method)
    auth_url: (String, String, String),
    // how many threads to test
    threads: i64,
    // how many threads per account use to test
    // total threads number is threads_per_account*accounts.len()
    threads_per_account: i64,
    // the result output to this path file
    output_file: Option<String>,
    
    headers: HashMap<String, String>,
    // ordinary params
    params: HashMap<String, String>,
    // thread_params used to distinct different params between each thread
    thread_params: HashMap<String, Box<Fn()->String>>,
    // modifier_params used to distinct different params between each request
    modifier_params: HashMap<String, T>,
    
    
    // accounts to simulate
    // <account_name, password>
    accounts: Vec<(String, String)>,
    left_values: (String, String, String),
    need_auth: bool
    
}

pub trait MtManagerTrait<T: MtModifierTrait + Default> {
    fn set_auth_url(&mut self, url: String, method: String, req_content_type: String) -> &mut Self;
    fn set_url(&mut self, url: String, method: String, req_content_type: String) -> &mut Self;
    fn set_seconds(&mut self, s: i64) -> &mut Self;
    fn set_threads(&mut self, n: i64) -> &mut Self;
    fn set_threads_per_account(&mut self, n: i64) -> &mut Self;
    fn add_account(&mut self, account: String, password: String) -> &mut Self;
    fn add_header(&mut self, key: String, value: String) -> &mut Self;
    fn add_param(&mut self, key: String, value: String) -> &mut Self;
    fn add_closure_param(&mut self, key: String, value: Box<Fn() -> String>) -> &mut Self;
    fn add_modifier_param(&mut self, key: String, value: T) -> &mut Self;
    
    // set the request content data type, default is www-form-urlencoded, you can set "urlencoded", or "json" now
    // fn set_param_type(&mut self, ctype: String) -> &mut Self;
    fn set_left_values(&mut self, account_key: String, pwd_key: String, left: String) -> &mut Self;
    fn output_file(&mut self, path: String) -> &mut Self;
    fn start(&mut self);
}


impl<T: MtModifierTrait + Default> MtManager<T> {
    
    pub fn new() -> MtManager<T> {
        let mut mt: MtManager<T> = Default::default();
        mt.thread_params = HashMap::<String, Box<Fn()->String>>::new();
        mt
    }
}


impl<T: MtModifierTrait + Default> MtManagerTrait<T> for MtManager<T> {
    fn set_auth_url(&mut self, url: String, method: String, req_content_type: String) -> &mut Self {
        self.auth_url = (url, method, req_content_type);
        self
    }
    
    fn set_url(&mut self, url: String, method: String, req_content_type: String) -> &mut Self {
        self.url = (url, method, req_content_type);
        self
    }
    
    fn set_seconds(&mut self, s: i64) -> &mut Self {
        self.time_seconds = s;
        self
    }
    
    fn set_threads(&mut self, n: i64) -> &mut Self {
        self.threads = n;
        self
    }
    
    fn set_threads_per_account(&mut self, n: i64) -> &mut Self {
        self.threads_per_account = n;
        self
    }
    
    fn add_account(&mut self, account: String, password: String) -> &mut Self {
        self.accounts.push((account, password));
        self
    }
    
    fn add_header(&mut self, key: String, value: String) -> &mut Self {
        self.headers.insert(key, value);
        self
    }
    
    fn add_param(&mut self, key: String, value: String) -> &mut Self {
        self.params.insert(key, value);
        self
    }
    
    fn add_closure_param(&mut self, key: String, closure: Box<Fn() -> String>) -> &mut Self {
        // excute this closure and insert its return value to hashmap
        // self.params.insert(key, closure());
        self.thread_params.insert(key, closure);
        self
    }
    
    fn add_modifier_param(&mut self, key: String, value: T) -> &mut Self {
        self.modifier_params.insert(key, value);
        self
    }
    
    // // set the request content data type, default is www-form-urlencoded, you can set "urlencoded", or "json" now
    // fn set_param_type(&mut self, ctype: String) -> &mut Self {
    //     self.req_content_type = ctype;
    //     self
    // }
    
    fn set_left_values(&mut self, account_key: String, pwd_key: String, left: String) -> &mut Self {
        self.left_values = (account_key, pwd_key, left);
        self
    }
    
    
    fn output_file(&mut self, path: String) -> &mut Self {
        // create path
        // let path_obj = Path::new(path);
        self.output_file = Some(path);
        self
    }
    
    
    fn start(&mut self) {
        // here now, self has been filled enough fileds to start with
        self.need_auth = if self.accounts.len() == 0 {
            false
        }
        else {
            true
        };
        
        let (tx, rx) = channel();
        let mut count = Arc::new(Mutex::new(0 as i64));
        
        if !self.need_auth {
            // consider no auth first
            for i in 0..self.threads {
                // prepare bindings and channel
                let (url, method, req_content_type) = self.url.clone();
                let time_seconds = self.time_seconds;
                let thread_tx = tx.clone();
                let headers = self.headers.clone();
                let mut params = self.params.clone();
                
                // calc thread_params to generate extra param for each thread
                for (key, clo) in &self.thread_params {
                    params.insert(key.clone(), clo());
                }
                
                let modifier_params = self.modifier_params.clone();
                let mut count = count.clone();
                
                // create self.threads threads, do loop in every thread
                thread::spawn ( move || {
                    _doreq(
                        thread_tx, 
                        method, 
                        url, 
                        headers, 
                        params, 
                        modifier_params,
                        req_content_type,
                        time_seconds,
                        count
                    );
                    
                    println!("thread {} finished.", i);
                });
                
            }
        }
        else {
            let accounts = self.accounts.clone();
            self.threads = accounts.len() as i64;
            for (account, pwd) in accounts {
                let (auth_url, auth_method, auth_req_type) = self.auth_url.clone();
                let (url, method, req_content_type) = self.url.clone();
                let left_values = self.left_values.clone();
                let time_seconds = self.time_seconds;
                let thread_tx = tx.clone();
                let headers = self.headers.clone();
                let mut params = self.params.clone();
                for (key, clo) in &self.thread_params {
                    params.insert(key.clone(), clo());
                }
                let modifier_params = self.modifier_params.clone();
                let mut count = count.clone();
                
                thread::spawn ( move || {
                    // auth first
                    let token = _doauth( 
                        auth_method.clone(), 
                        auth_url.clone(), 
                        (account.clone(), pwd.clone()),
                        HashMap::new(), 
                        HashMap::new(), 
                        auth_req_type.clone(),
                        left_values);
                        
                    // here, we should do _doreq
                    // TODO: attache headers and params, req_content_type, method to each url, if we have more than one urls
                    let mut headers = headers;
                    headers.insert("Authorization".to_string(), token);
                    
                    _doreq(
                        thread_tx, 
                        method, 
                        url, 
                        headers, 
                        params, 
                        modifier_params,
                        req_content_type,
                        time_seconds,
                        count
                    );
                    
                    println!("thread {} finished.", account);
                });
            }
            
        }
        
        
        // in main thread, collect the return result, and calculate
        let mut collectors = Vec::with_capacity(self.threads as usize);
        for i in 0..self.threads {
            collectors.push(rx.recv().unwrap());
        }
        
        let total_requests = collectors.iter().fold(0, |acc, ref item| acc + item.len());
        let table = table!(
            ["Auth Url", format!("{} {}", self.auth_url.1, self.auth_url.0)],
            ["Urls", format!("{} {}", self.url.1, self.url.0)],
            ["Headers", format!("{:#?}", self.headers)],
            ["Params", format!("{:#?}", self.params)],
            ["Time Last", self.time_seconds],
            ["Users", self.threads],
            ["Total RPS", format!("{:.2}", total_requests as f64 / self.time_seconds as f64)]
        );
        table.printstd();
        
        thread::sleep_ms(1000);
        
    }
}

#[derive(Debug)]
struct ReqResult {
    // response status
    status: hyper::status::StatusCode,
    // response body length
    body_length: i64,
    // req->res duration, m seconds
    time_last: f64,
    // a string after processing
    restext: String
}

fn _do_get(client: Arc<Client>, url: String, headers: HashMap<String, String>, params: HashMap<String, String>) -> Response {
    // fill neccessary headers
    let mut headers_obj = Headers::new();
    for (key, val) in headers {
        headers_obj.set_raw(key, vec![val.as_bytes().to_vec()]);
    }
    let query_string = url_m::form_urlencoded::serialize(params);
    // println!("query_string is: {}", query_string);
    let cres = client.get(&(url + "?" + &query_string) )
        .headers(headers_obj)
        .send().unwrap();
        
    cres
}

fn _do_post(client: Arc<Client>, url: String, headers: HashMap<String, String>, params: HashMap<String, String>, req_content_type: String) -> Response {
    // fill neccessary headers
    let mut body_string;
    let mut headers_obj = Headers::new();
    // set more custom headers
    for (key, val) in headers {
        headers_obj.set_raw(key, vec![val.as_bytes().to_vec()]);
    }
    
    if req_content_type == "json".to_owned() {
        // set json content type headers
        headers_obj.set(ContentType::json());
        
        let json_body = jsonway::object(|json| {
            for (key, val) in params {
                json.set(key.to_owned(), val.to_owned());
            }
        }).unwrap();
        
        body_string = json_body.to_string();
        println!("{}", body_string);
    }
    else {
        // urlencoded params
        body_string = url_m::form_urlencoded::serialize(params);
    }
    
    let mut cres = client.post(&url)
        .headers(headers_obj)
        .body(&body_string)
        .send().unwrap();
        
    cres
}

fn _doreq<T: MtModifierTrait> (
        thread_tx: Sender<Vec<ReqResult>>,
        method: String, 
        url: String, 
        headers: HashMap<String, String>, 
        params: HashMap<String, String>, 
        modifier_params: HashMap<String, T>, 
        req_content_type: String,
        time_seconds: i64,
        count: Arc<Mutex<i64>>
        ) {
    let mut bench_result: Vec<ReqResult> = vec![];
    
    // using hyper to do http client request
    let mut client = Arc::new(Client::new());
    
    // calculate current timestamp;
    let start_t = time::precise_time_s();
    
    loop {
        let mut n_count = 0;
        {
            // increase the counter at the begin of a request start
            let mut count = count.lock().unwrap();
            *count += 1;
            println!("times: {}", *count);
            n_count = *count;
        }
        
        let mut params = params.clone();
        for (key, val) in &modifier_params {
            // execute the trans method of that modifier
            params.insert(key.clone(), val.before(n_count));
        }
        
        let per_start = time::precise_time_ns();
        
        let mut cres;
        if method == "GET".to_owned() {
            cres = _do_get(client.clone(), url.clone(), headers.clone(), params.clone());
        }
        else {
            cres = _do_post(client.clone(), url.clone(), headers.clone(), params.clone(), req_content_type.clone());
        }
        let mut cbody = String::new();
        cres.read_to_string(&mut cbody).unwrap();
        // println!("ret value: {}", cbody);
        
        let per_end = time::precise_time_ns();

        let nbody = cbody.len();
        
        let mut restext = String::new();
        for (_, val) in &modifier_params {
            // execute the trans method of that modifier, and collect the postprocess result
            restext.push_str(&val.after(n_count, &cbody));
        }
        
        // assert_eq!(cres.status, hyper::Ok);
        // make ReqResult instance
        let req_result = ReqResult {
            status: cres.status,
            body_length: nbody as i64,
            time_last: (per_end - per_start) as f64 / 1000000.0,
            restext: restext
        };
        // println!("h->{:?}", headers.clone());
        // println!("r->{:?}", req_result);
        bench_result.push(req_result);
        
        
        let end_t = time::precise_time_s ();
        let delta = end_t - start_t;
        // check the time duration, if exceed, jump out
        if delta >= time_seconds as f64 {
            // send bench_result to main thread using channel
            thread_tx.send(bench_result).unwrap();
            
            // jump out
            break;
        }
    }
    
}

///
///
///
fn _doauth (
        method: String, 
        url: String, 
        account: (String, String),
        headers: HashMap<String, String>, 
        params: HashMap<String, String>, 
        req_content_type: String,
        left_values: (String, String, String) ) -> String {
    
    // using hyper to do http client request
    let mut client = Arc::new(Client::new());
    
    // calculate current timestamp;
    let start_t = time::precise_time_ns();
    let mut cres;
    if method == "GET".to_owned() {
        cres = _do_get(client.clone(), url, headers, params)
    }
    else {

        let mut params = params;
        params.insert(left_values.0, account.0.clone());
        params.insert(left_values.1, account.1);
        
        cres = _do_post(client.clone(), url, headers, params, req_content_type);
    }
    let mut cbody = String::new();
    cres.read_to_string(&mut cbody).unwrap();
    // println!("ret is: {}", cbody);
    
    let end_t = time::precise_time_ns();
    
    let json_result = Json::from_str(&cbody[..]).unwrap();
    
    let token = json_result.find(&left_values.2).unwrap().as_string().unwrap();
    
    println!("user {} token is {}", account.0, token);
    
    // next, we should use this token to put to headers for next requests
    
    token.to_owned()
}

