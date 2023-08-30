mod attr;
mod parse_box;
mod parse_boxes;
mod parsed_box;

use synstructure::decl_derive;

decl_derive!([ParseBox, attributes(box_type)] => parse_box::derive);
decl_derive!([ParsedBox, attributes(box_type)] => parsed_box::derive);
decl_derive!([ParseBoxes, attributes(box_type)] => parse_boxes::derive);
