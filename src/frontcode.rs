use std::collections::HashMap;

use ic_cdk_macros::{update, query};
use ic_cdk::{
    export::candid::{CandidType, Deserialize, Func},
    api::{data_certificate, set_certified_data}
};
use ic_certified_map::{RbTree, HashTree, AsHashTree};

use serde::Serialize;

use crate::tools::{
    sha256
};
use crate::stable::{
    put_file_hashes,
    get_file_hashes,
    put_files,
    get_files,
    // put_file,
    // get_file
};




const LABEL_ASSETS: &[u8; 11] = b"http_assets";

#[derive(CandidType, Deserialize)]
pub struct File {
    content_type: String,
    content_encoding: String,
    content: Box<[u8]>
}
pub type Files = HashMap<String, File>;
pub type FileHashes = RbTree<String, ic_certified_map::Hash>;



#[derive(Clone, Debug, CandidType, Deserialize)]
struct Token {}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum StreamingStrategy {
    Callback { callback: Func, token: Token},
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct StreamingCallbackHttpResponse {
    body: Vec<u8>,
    token: Option<Token>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct HttpRequest {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct HttpResponse {
    status_code: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    streaming_strategy: Option<StreamingStrategy>,
}








fn set_root_hash(tree: &FileHashes) {
    let root_hash = ic_certified_map::labeled_hash(LABEL_ASSETS, &tree.root_hash());
    set_certified_data(&root_hash[..]);
}


fn make_file_certificate_header(file_name: &str) -> (String, String) {
    let certificate: Vec<u8> = data_certificate().unwrap();
    let file_hashes: FileHashes = get_file_hashes();
    let witness: HashTree = file_hashes.witness(file_name.as_bytes());
    let tree: HashTree = ic_certified_map::labeled(LABEL_ASSETS, witness);
    let mut serializer = serde_cbor::ser::Serializer::new(vec![]);
    serializer.self_describe().unwrap();
    tree.serialize(&mut serializer).unwrap();
    (
        "IC-Certificate".to_string(),
        format!("certificate=:{}:, tree=:{}:",
            base64::encode(&certificate),
            base64::encode(&serializer.into_inner())
        )
    )
}



#[update]
pub fn upload_frontcode_file_chunks(file_path: String, file: File) -> () {
    let mut file_hashes: FileHashes = get_file_hashes();
    file_hashes.insert(file_path.clone(), sha256(&file.content));
    put_file_hashes(&file_hashes);
    set_root_hash(&file_hashes);

    let mut files: Files = get_files();
    files.insert(file_path, file);
    put_files(&files);

    
}


#[query]
pub fn http_request(quest: HttpRequest) -> HttpResponse {
    let file_name = quest.url;
    let files: Files = get_files();
    let file: &File = files.get(&file_name).unwrap();
    let certificate_header: (String, String) = make_file_certificate_header(&file_name);
    
    HttpResponse {
        status_code: 200,
        headers: vec![
            certificate_header, 
            ("content-type".to_string(), file.content_type.clone()),
            ("content-encoding".to_string(), file.content_encoding.clone())
        ],
        body: file.content.to_vec(),
        streaming_strategy: None
    }
}


#[query]
pub fn public_get_file_hashes() -> Vec<(String, [u8; 32])> {
    let file_hashes = get_file_hashes();
    let mut vec = Vec::<(String, [u8; 32])>::new();
    file_hashes.for_each(|k,v| {
        vec.push((std::str::from_utf8(k).unwrap().to_string(), *v));
    });
    vec
}


#[update]
pub fn public_clear_file_hashes() {
    put_file_hashes(&FileHashes::default());
}
