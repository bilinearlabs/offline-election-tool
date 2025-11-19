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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_block() {
        let block = parse_block(Some("0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef".to_string()));
        assert!(block.is_ok());
        let block = block.unwrap();
        assert!(block.is_some());
        let block = block.unwrap();
        assert_eq!(block, H256::from_str("0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef").unwrap());
    }

    #[test]
    fn test_parse_block_latest() {
        let block = parse_block(Some("latest".to_string()));
        assert!(block.is_ok());
        let block = block.unwrap();
        assert!(block.is_none());
    }

    #[test]
    fn test_parse_block_invalid() {
        let block = parse_block(Some("invalid".to_string()));
        assert!(block.is_err());
    }

    #[test]
    fn test_parse_block_none() {
        let block = parse_block(None);
        assert!(block.is_ok());
        let block = block.unwrap();
        assert!(block.is_none());
    }
}