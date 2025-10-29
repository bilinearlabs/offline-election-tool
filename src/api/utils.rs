use std::str::FromStr;

use sp_core::H256;

pub fn parse_block(block_str: Option<String>) -> Result<Option<H256>, Box<dyn std::error::Error>> {
    if block_str.is_none() {
        return Ok(None);
    }
    let block_str = block_str.unwrap();
    if block_str == "latest" {
        return Ok(None);
    }
    let block = H256::from_str(block_str.as_str());
    if block.is_err() {
        return Err(format!("Invalid block: {}", block.err().unwrap()).into());
    }
    let block = block.unwrap();
    Ok(Some(block))
}
