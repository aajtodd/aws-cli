//! ARN parsing shared by URI handling.

/// A parsed Amazon Resource Name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arn {
    pub partition: String,
    pub service: String,
    pub region: String,
    pub account: String,
    pub resource: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ArnParseError {
    #[error("ARN must have 6 colon-separated fields: {0}")]
    WrongFieldCount(String),
    #[error("ARN must start with `arn:`: {0}")]
    MissingArnPrefix(String),
    #[error("ARN partition is empty: {0}")]
    EmptyPartition(String),
    #[error("ARN service is empty: {0}")]
    EmptyService(String),
}

impl Arn {
    /// Parse an ARN string. Accepts anything that looks structurally like an ARN
    /// (6 colon-separated fields starting with `arn`); service-level validation
    /// happens at the caller.
    ///
    /// Note: the `resource` field is kept raw because S3 ARNs use mixed `/` and `:`
    /// separators inside the resource path. Shape-specific parsing happens in uri.rs.
    pub fn parse(s: &str) -> Result<Self, ArnParseError> {
        // Split into AT MOST 6 fields so the resource field retains any embedded `:`.
        let parts: Vec<&str> = s.splitn(6, ':').collect();
        if parts.len() != 6 {
            return Err(ArnParseError::WrongFieldCount(s.to_string()));
        }
        if parts[0] != "arn" {
            return Err(ArnParseError::MissingArnPrefix(s.to_string()));
        }
        if parts[1].is_empty() {
            return Err(ArnParseError::EmptyPartition(s.to_string()));
        }
        if parts[2].is_empty() {
            return Err(ArnParseError::EmptyService(s.to_string()));
        }
        Ok(Arn {
            partition: parts[1].to_string(),
            service: parts[2].to_string(),
            region: parts[3].to_string(),
            account: parts[4].to_string(),
            resource: parts[5].to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_standard_arn() {
        let arn = Arn::parse("arn:aws:s3:us-east-1:123456789012:accesspoint/my-ap").unwrap();
        assert_eq!(arn.partition, "aws");
        assert_eq!(arn.service, "s3");
        assert_eq!(arn.region, "us-east-1");
        assert_eq!(arn.account, "123456789012");
        assert_eq!(arn.resource, "accesspoint/my-ap");
    }

    #[test]
    fn valid_arn_empty_region() {
        let arn = Arn::parse("arn:aws:s3::123456789012:accesspoint/mrap-alias").unwrap();
        assert_eq!(arn.region, "");
    }

    #[test]
    fn valid_arn_colon_in_resource() {
        let arn = Arn::parse("arn:aws:s3:us-east-1:123456789012:accesspoint:my-ap").unwrap();
        assert_eq!(arn.resource, "accesspoint:my-ap");
    }

    #[test]
    fn non_arn_string() {
        let err = Arn::parse("foo").unwrap_err();
        assert_eq!(err, ArnParseError::WrongFieldCount("foo".to_string()));
    }

    #[test]
    fn wrong_prefix() {
        let err = Arn::parse("xrn:aws:s3:us-east-1:123:ap").unwrap_err();
        assert_eq!(
            err,
            ArnParseError::MissingArnPrefix("xrn:aws:s3:us-east-1:123:ap".to_string())
        );
    }

    #[test]
    fn empty_partition() {
        let err = Arn::parse("arn::s3:us-east-1:123:ap").unwrap_err();
        assert_eq!(
            err,
            ArnParseError::EmptyPartition("arn::s3:us-east-1:123:ap".to_string())
        );
    }

    #[test]
    fn empty_service() {
        let err = Arn::parse("arn:aws::us-east-1:123:ap").unwrap_err();
        assert_eq!(
            err,
            ArnParseError::EmptyService("arn:aws::us-east-1:123:ap".to_string())
        );
    }
}
