// Generated from glibc's iconv tables (IBM037 ... IBM1149): what EBCDIC
// bytes 0x40..=0xFF show as, one char each -- for each byte b,
// `printf '\xNN' | iconv -f IBMnnn -t UTF-32LE`. 0x00..=0x3F are 5250
// controls and attributes, never text; controls in the upper half show
// as blanks. U+00A0 and U+00AD are written as escapes (clippy:
// invisible_characters).

pub(super) const CP037: &str = " \u{a0}âäàáãåçñ¢.<(+|&éêëèíîïìß!$*);¬-/ÂÄÀÁÃÅÇÑ¦,%_>?øÉÊËÈÍÎÏÌ`:#@'=\"Øabcdefghi«»ðýþ±°jklmnopqrªºæ¸Æ¤µ~stuvwxyz¡¿ÐÝÞ®^£¥·©§¶¼½¾[]¯¨´×{ABCDEFGHI\u{ad}ôöòóõ}JKLMNOPQR¹ûüùúÿ\\÷STUVWXYZ²ÔÖÒÓÕ0123456789³ÛÜÙÚ ";
pub(super) const CP273: &str = " \u{a0}â{àáãåçñÄ.<(+!&éêëèíîïì~Ü$*);^-/Â[ÀÁÃÅÇÑö,%_>?øÉÊËÈÍÎÏÌ`:#§'=\"Øabcdefghi«»ðýþ±°jklmnopqrªºæ¸Æ¤µßstuvwxyz¡¿ÐÝÞ®¢£¥·©@¶¼½¾¬|¯¨´×äABCDEFGHI\u{ad}ô¦òóõüJKLMNOPQR¹û}ùúÿÖ÷STUVWXYZ²Ô\\ÒÓÕ0123456789³Û]ÙÚ ";
pub(super) const CP277: &str = " \u{a0}âäàáã}çñ#.<(+!&éêëèíîïìß¤Å*);^-/ÂÄÀÁÃ$ÇÑø,%_>?¦ÉÊËÈÍÎÏÌ`:ÆØ'=\"@abcdefghi«»ðýþ±°jklmnopqrªº{¸[]µüstuvwxyz¡¿ÐÝÞ®¢£¥·©§¶¼½¾¬|¯¨´×æABCDEFGHI\u{ad}ôöòóõåJKLMNOPQR¹û~ùúÿ\\÷STUVWXYZ²ÔÖÒÓÕ0123456789³ÛÜÙÚ ";
pub(super) const CP278: &str = " \u{a0}â{àáã}çñ§.<(+!&`êëèíîïìß¤Å*);^-/Â#ÀÁÃ$ÇÑö,%_>?øÉÊËÈÍÎÏÌé:ÄÖ'=\"Øabcdefghi«»ðýþ±°jklmnopqrªºæ¸Æ]µüstuvwxyz¡¿ÐÝÞ®¢£¥·©[¶¼½¾¬|¯¨´×äABCDEFGHI\u{ad}ô¦òóõåJKLMNOPQR¹û~ùúÿ\\÷STUVWXYZ²Ô@ÒÓÕ0123456789³ÛÜÙÚ ";
pub(super) const CP280: &str = " \u{a0}âä{áãå\\ñ°.<(+!&]êë}íîï~ßé$*);^-/ÂÄÀÁÃÅÇÑò,%_>?øÉÊËÈÍÎÏÌù:£§'=\"Øabcdefghi«»ðýþ±[jklmnopqrªºæ¸Æ¤µìstuvwxyz¡¿ÐÝÞ®¢#¥·©@¶¼½¾¬|¯¨´×àABCDEFGHI\u{ad}ôö¦óõèJKLMNOPQR¹ûü`úÿç÷STUVWXYZ²ÔÖÒÓÕ0123456789³ÛÜÙÚ ";
pub(super) const CP284: &str = " \u{a0}âäàáãåç¦[.<(+|&éêëèíîïìß]$*);¬-/ÂÄÀÁÃÅÇ#ñ,%_>?øÉÊËÈÍÎÏÌ`:Ñ@'=\"Øabcdefghi«»ðýþ±°jklmnopqrªºæ¸Æ¤µ¨stuvwxyz¡¿ÐÝÞ®¢£¥·©§¶¼½¾^!¯~´×{ABCDEFGHI\u{ad}ôöòóõ}JKLMNOPQR¹ûüùúÿ\\÷STUVWXYZ²ÔÖÒÓÕ0123456789³ÛÜÙÚ ";
pub(super) const CP285: &str = " \u{a0}âäàáãåçñ$.<(+|&éêëèíîïìß!£*);¬-/ÂÄÀÁÃÅÇÑ¦,%_>?øÉÊËÈÍÎÏÌ`:#@'=\"Øabcdefghi«»ðýþ±°jklmnopqrªºæ¸Æ¤µ‾stuvwxyz¡¿ÐÝÞ®¢[¥·©§¶¼½¾^]~¨´×{ABCDEFGHI\u{ad}ôöòóõ}JKLMNOPQR¹ûüùúÿ\\÷STUVWXYZ²ÔÖÒÓÕ0123456789³ÛÜÙÚ ";
pub(super) const CP297: &str = " \u{a0}âä@áãå\\ñ°.<(+!&{êë}íîïìß§$*);^-/ÂÄÀÁÃÅÇÑù,%_>?øÉÊËÈÍÎÏÌµ:£à'=\"Øabcdefghi«»ðýþ±[jklmnopqrªºæ¸Æ¤`¨stuvwxyz¡¿ÐÝÞ®¢#¥·©]¶¼½¾¬|¯~´×éABCDEFGHI\u{ad}ôöòóõèJKLMNOPQR¹ûü¦úÿç÷STUVWXYZ²ÔÖÒÓÕ0123456789³ÛÜÙÚ ";
pub(super) const CP500: &str = " \u{a0}âäàáãåçñ[.<(+!&éêëèíîïìß]$*);^-/ÂÄÀÁÃÅÇÑ¦,%_>?øÉÊËÈÍÎÏÌ`:#@'=\"Øabcdefghi«»ðýþ±°jklmnopqrªºæ¸Æ¤µ~stuvwxyz¡¿ÐÝÞ®¢£¥·©§¶¼½¾¬|¯¨´×{ABCDEFGHI\u{ad}ôöòóõ}JKLMNOPQR¹ûüùúÿ\\÷STUVWXYZ²ÔÖÒÓÕ0123456789³ÛÜÙÚ ";
pub(super) const CP871: &str = " \u{a0}âäàáãåçñþ.<(+!&éêëèíîïìßÆ$*);Ö-/ÂÄÀÁÃÅÇÑ¦,%_>?øÉÊËÈÍÎÏÌð:#Ð'=\"Øabcdefghi«»`ý{±°jklmnopqrªº}¸]¤µöstuvwxyz¡¿@Ý[®¢£¥·©§¶¼½¾¬|¯¨\\×ÞABCDEFGHI\u{ad}ô~òóõæJKLMNOPQR¹ûüùúÿ´÷STUVWXYZ²Ô^ÒÓÕ0123456789³ÛÜÙÚ ";
pub(super) const CP1140: &str = " \u{a0}âäàáãåçñ¢.<(+|&éêëèíîïìß!$*);¬-/ÂÄÀÁÃÅÇÑ¦,%_>?øÉÊËÈÍÎÏÌ`:#@'=\"Øabcdefghi«»ðýþ±°jklmnopqrªºæ¸Æ€µ~stuvwxyz¡¿ÐÝÞ®^£¥·©§¶¼½¾[]¯¨´×{ABCDEFGHI\u{ad}ôöòóõ}JKLMNOPQR¹ûüùúÿ\\÷STUVWXYZ²ÔÖÒÓÕ0123456789³ÛÜÙÚ ";
pub(super) const CP1141: &str = " \u{a0}â{àáãåçñÄ.<(+!&éêëèíîïì~Ü$*);^-/Â[ÀÁÃÅÇÑö,%_>?øÉÊËÈÍÎÏÌ`:#§'=\"Øabcdefghi«»ðýþ±°jklmnopqrªºæ¸Æ€µßstuvwxyz¡¿ÐÝÞ®¢£¥·©@¶¼½¾¬|¯¨´×äABCDEFGHI\u{ad}ô¦òóõüJKLMNOPQR¹û}ùúÿÖ÷STUVWXYZ²Ô\\ÒÓÕ0123456789³Û]ÙÚ ";
pub(super) const CP1142: &str = " \u{a0}âäàáã}çñ#.<(+!&éêëèíîïìß€Å*);^-/ÂÄÀÁÃ$ÇÑø,%_>?¦ÉÊËÈÍÎÏÌ`:ÆØ'=\"@abcdefghi«»ðýþ±°jklmnopqrªº{¸[]µüstuvwxyz¡¿ÐÝÞ®¢£¥·©§¶¼½¾¬|¯¨´×æABCDEFGHI\u{ad}ôöòóõåJKLMNOPQR¹û~ùúÿ\\÷STUVWXYZ²ÔÖÒÓÕ0123456789³ÛÜÙÚ ";
pub(super) const CP1143: &str = " \u{a0}â{àáã}çñ§.<(+!&`êëèíîïìß€Å*);^-/Â#ÀÁÃ$ÇÑö,%_>?ø\\ÊËÈÍÎÏÌé:ÄÖ'=\"Øabcdefghi«»ðýþ±°jklmnopqrªºæ¸Æ]µüstuvwxyz¡¿ÐÝÞ®¢£¥·©[¶¼½¾¬|¯¨´×äABCDEFGHI\u{ad}ô¦òóõåJKLMNOPQR¹û~ùúÿÉ÷STUVWXYZ²Ô@ÒÓÕ0123456789³ÛÜÙÚ ";
pub(super) const CP1144: &str = " \u{a0}âä{áãå\\ñ°.<(+!&]êë}íîï~ßé$*);^-/ÂÄÀÁÃÅÇÑò,%_>?øÉÊËÈÍÎÏÌù:£§'=\"Øabcdefghi«»ðýþ±[jklmnopqrªºæ¸Æ€µìstuvwxyz¡¿ÐÝÞ®¢#¥·©@¶¼½¾¬|¯¨´×àABCDEFGHI\u{ad}ôö¦óõèJKLMNOPQR¹ûü`úÿç÷STUVWXYZ²ÔÖÒÓÕ0123456789³ÛÜÙÚ ";
pub(super) const CP1145: &str = " \u{a0}âäàáãåç¦[.<(+|&éêëèíîïìß]$*);¬-/ÂÄÀÁÃÅÇ#ñ,%_>?øÉÊËÈÍÎÏÌ`:Ñ@'=\"Øabcdefghi«»ðýþ±°jklmnopqrªºæ¸Æ€µ¨stuvwxyz¡¿ÐÝÞ®¢£¥·©§¶¼½¾^!¯~´×{ABCDEFGHI\u{ad}ôöòóõ}JKLMNOPQR¹ûüùúÿ\\÷STUVWXYZ²ÔÖÒÓÕ0123456789³ÛÜÙÚ ";
pub(super) const CP1146: &str = " \u{a0}âäàáãåçñ$.<(+|&éêëèíîïìß!£*);¬-/ÂÄÀÁÃÅÇÑ¦,%_>?øÉÊËÈÍÎÏÌ`:#@'=\"Øabcdefghi«»ðýþ±°jklmnopqrªºæ¸Æ€µ¯stuvwxyz¡¿ÐÝÞ®¢[¥·©§¶¼½¾^]~¨´×{ABCDEFGHI\u{ad}ôöòóõ}JKLMNOPQR¹ûüùúÿ\\÷STUVWXYZ²ÔÖÒÓÕ0123456789³ÛÜÙÚ ";
pub(super) const CP1147: &str = " \u{a0}âä@áãå\\ñ°.<(+!&{êë}íîïìß§$*);^-/ÂÄÀÁÃÅÇÑù,%_>?øÉÊËÈÍÎÏÌµ:£à'=\"Øabcdefghi«»ðýþ±[jklmnopqrªºæ¸Æ€`¨stuvwxyz¡¿ÐÝÞ®¢#¥·©]¶¼½¾¬|¯~´×éABCDEFGHI\u{ad}ôöòóõèJKLMNOPQR¹ûü¦úÿç÷STUVWXYZ²ÔÖÒÓÕ0123456789³ÛÜÙÚ ";
pub(super) const CP1148: &str = " \u{a0}âäàáãåçñ[.<(+!&éêëèíîïìß]$*);^-/ÂÄÀÁÃÅÇÑ¦,%_>?øÉÊËÈÍÎÏÌ`:#@'=\"Øabcdefghi«»ðýþ±°jklmnopqrªºæ¸Æ€µ~stuvwxyz¡¿ÐÝÞ®¢£¥·©§¶¼½¾¬|¯¨´×{ABCDEFGHI\u{ad}ôöòóõ}JKLMNOPQR¹ûüùúÿ\\÷STUVWXYZ²ÔÖÒÓÕ0123456789³ÛÜÙÚ ";
pub(super) const CP1149: &str = " \u{a0}âäàáãåçñÞ.<(+!&éêëèíîïìßÆ$*);Ö-/ÂÄÀÁÃÅÇÑ¦,%_>?øÉÊËÈÍÎÏÌð:#Ð'=\"Øabcdefghi«»`ý{±°jklmnopqrªº}¸]€µöstuvwxyz¡¿@Ý[®¢£¥·©§¶¼½¾¬|¯¨\\×þABCDEFGHI\u{ad}ô~òóõæJKLMNOPQR¹ûüùúÿ´÷STUVWXYZ²Ô^ÒÓÕ0123456789³ÛÜÙÚ ";
