use pun_format::Fixer;
use pun_tokens::Kind::*;
use pun_tokens::{Token, Tokens};
use std::path::Path;

struct PreferDedicatedStatusAssertion;

pub(crate) fn fixer() -> Box<dyn Fixer> {
    Box::new(PreferDedicatedStatusAssertion)
}

impl Fixer for PreferDedicatedStatusAssertion {
    fn name(&self) -> &'static str {
        "Pun/prefer_dedicated_status_assertion"
    }
    fn priority(&self) -> i32 {
        0
    }
    fn is_candidate(&self, tokens: &Tokens) -> bool {
        tokens.is_all_token_kinds_found(&[T_OBJECT_OPERATOR, T_LNUMBER])
    }
    fn fix(&self, _file: &Path, tokens: &mut Tokens) {
        for i in (1..tokens.count()).rev() {
            if !tokens[i].is_kind(T_STRING)
                || !tokens[i]
                    .get_content()
                    .eq_ignore_ascii_case(b"assertStatus")
            {
                continue;
            }
            if !tokens
                .get_prev_meaningful_token(i)
                .is_some_and(|p| tokens[p].is_kind(T_OBJECT_OPERATOR))
            {
                continue;
            }
            let Some(open) = tokens
                .get_next_meaningful_token(i)
                .filter(|&o| tokens[o].equals("(", true))
            else {
                continue;
            };
            let Some(num) = tokens
                .get_next_meaningful_token(open)
                .filter(|&n| tokens[n].is_kind(T_LNUMBER))
            else {
                continue;
            };
            let mut close = tokens.get_next_meaningful_token(num).unwrap();
            if tokens[close].equals(",", true) {
                close = tokens.get_next_meaningful_token(close).unwrap();
            }
            if !tokens[close].equals(")", true) || (i..close).any(|k| tokens[k].is_comment()) {
                continue;
            }
            let Some(method) =
                int_literal_value(tokens[num].get_content()).and_then(dedicated_status_method)
            else {
                continue;
            };
            tokens.override_range(
                i,
                close,
                vec![
                    Token::new(T_STRING, method),
                    Token::char("("),
                    Token::char(")"),
                ],
            );
        }
    }
}

fn int_literal_value(literal: &[u8]) -> Option<u64> {
    let digits: String = literal
        .iter()
        .filter(|&&b| b != b'_')
        .map(|&b| b as char)
        .collect();
    let (radix, digits) = match digits.as_bytes() {
        [b'0', b'x' | b'X', ..] => (16, &digits[2..]),
        [b'0', b'b' | b'B', ..] => (2, &digits[2..]),
        [b'0', b'o' | b'O', ..] => (8, &digits[2..]),
        [b'0', _, ..] => (8, &digits[1..]),
        _ => (10, &digits[..]),
    };
    u64::from_str_radix(digits, radix).ok()
}

fn dedicated_status_method(status_code: u64) -> Option<&'static str> {
    Some(match status_code {
        200 => "assertOk",
        201 => "assertCreated",
        202 => "assertAccepted",
        204 => "assertNoContent",
        301 => "assertMovedPermanently",
        302 => "assertFound",
        304 => "assertNotModified",
        307 => "assertTemporaryRedirect",
        308 => "assertPermanentRedirect",
        400 => "assertBadRequest",
        401 => "assertUnauthorized",
        402 => "assertPaymentRequired",
        403 => "assertForbidden",
        404 => "assertNotFound",
        405 => "assertMethodNotAllowed",
        406 => "assertNotAcceptable",
        408 => "assertRequestTimeout",
        409 => "assertConflict",
        410 => "assertGone",
        415 => "assertUnsupportedMediaType",
        422 => "assertUnprocessable",
        424 => "assertFailedDependency",
        429 => "assertTooManyRequests",
        500 => "assertInternalServerError",
        503 => "assertServiceUnavailable",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    fn fix(code: &str) -> String {
        crate::fix_with(&*super::fixer(), code)
    }

    #[test]
    fn mago_cases() {
        assert_eq!(
            fix("<?php\n\n$response->assertStatus(404);\n"),
            "<?php\n\n$response->assertNotFound();\n"
        );
        assert_eq!(
            fix("<?php\n\n$response->assertStatus(200);\n"),
            "<?php\n\n$response->assertOk();\n"
        );
        assert_eq!(
            fix("<?php\n\n$this->get('/missing')->assertStatus(404);\n"),
            "<?php\n\n$this->get('/missing')->assertNotFound();\n"
        );
        for same in [
            "<?php\n\n$response->assertOk();\n",
            "<?php\n\n$response->assertStatus(599);\n",
            "<?php\n\n$response->assertStatus($code);\n",
        ] {
            assert_eq!(fix(same), same);
        }
    }

    #[test]
    fn laravel_status_methods() {
        let laravel = [
            (200, "assertOk"),
            (201, "assertCreated"),
            (202, "assertAccepted"),
            (204, "assertNoContent"),
            (301, "assertMovedPermanently"),
            (302, "assertFound"),
            (304, "assertNotModified"),
            (307, "assertTemporaryRedirect"),
            (308, "assertPermanentRedirect"),
            (400, "assertBadRequest"),
            (401, "assertUnauthorized"),
            (402, "assertPaymentRequired"),
            (403, "assertForbidden"),
            (404, "assertNotFound"),
            (405, "assertMethodNotAllowed"),
            (406, "assertNotAcceptable"),
            (408, "assertRequestTimeout"),
            (409, "assertConflict"),
            (410, "assertGone"),
            (415, "assertUnsupportedMediaType"),
            (422, "assertUnprocessable"),
            (424, "assertFailedDependency"),
            (429, "assertTooManyRequests"),
            (500, "assertInternalServerError"),
            (503, "assertServiceUnavailable"),
        ];
        for (code, method) in laravel {
            assert_eq!(
                fix(&format!("<?php $r->assertStatus({code});")),
                format!("<?php $r->{method}();")
            );
        }
        assert_eq!(
            fix("<?php $r->assertStatus(4_04);"),
            "<?php $r->assertNotFound();"
        );
        for literal in [
            "0624",
            "0o624",
            "0O624",
            "0x194",
            "0X1_94",
            "0b110010100",
            "0B110010100",
        ] {
            assert_eq!(
                fix(&format!("<?php $r->assertStatus({literal});")),
                "<?php $r->assertNotFound();"
            );
        }
        assert_eq!(
            fix("<?php $r->assertStatus(0404);"),
            "<?php $r->assertStatus(0404);"
        );
    }

    #[test]
    fn edge_cases() {
        assert_eq!(
            fix("<?php $r\n    ->ASSERTSTATUS( 201, );"),
            "<?php $r\n    ->assertCreated();"
        );
        for same in [
            "<?php $r->assertStatus(200, 'msg');",
            "<?php $r->assertStatus(200 + 1);",
            "<?php $r->assertStatus(/* ok */ 200);",
            "<?php Foo::assertStatus(200);",
            "<?php $a->b; Foo::assertStatus(200);",
            "<?php $r?->assertStatus(200);",
            "<?php $r->assertStatus; $x = 1;",
            "<?php $r->assertStatus($c); $x = 1;",
        ] {
            assert_eq!(fix(same), same);
        }
    }
}
