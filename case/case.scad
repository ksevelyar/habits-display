include <mixin.scad>;

display_lcd_length = 39;
display_lcd_width = 30.1;

display_pcb_length = 37;
display_pcb_width = 30;

lcd_cutout_z = 0.7;
lcd_cutout_h = 3.1;
pcb_cutout_z = -0.1;
pcb_cutout_h = 1;

screw_inset = 2.1;
screw_top_d = 2.1;
screw_bot_d = 1.8;
screw_h = 3;

$fn = 128;

module display_cutout() {
  translate(
    [
      (case_width - display_lcd_width) / 2,
      (case_length - display_lcd_length) / 2,
      lcd_cutout_z,
    ]
  ) cube([display_lcd_width, display_lcd_length, lcd_cutout_h]);

  translate(
    [
      (case_width - display_pcb_width) / 2,
      (case_length - display_pcb_length) / 2,
      pcb_cutout_z,
    ]
  )
    rounded_box(size=[display_pcb_width, display_pcb_length, pcb_cutout_h], r=5);

  translate(
    [
      (case_width - display_lcd_width) / 2 + 3.5,
      (case_length - display_lcd_length) / 2 - 3.5,
      lcd_cutout_z,
    ]
  ) cube([display_pcb_width - 7, 2, lcd_cutout_h]);

  translate([wall + screw_inset, wall + screw_inset, 0]) cylinder(screw_h, d1=screw_top_d, d2=screw_bot_d);
  translate([case_width - screw_inset - wall, wall + screw_inset, 0]) cylinder(screw_h, d1=screw_top_d, d2=screw_bot_d);
  translate([case_width - screw_inset - wall, case_length - screw_inset - wall, 0]) cylinder(screw_h, d1=screw_top_d, d2=screw_bot_d);
  translate([wall + screw_inset, case_length - screw_inset - wall, 0]) cylinder(screw_h, d1=screw_top_d, d2=screw_bot_d);
}

module base() {
  difference() {
    rounded_wedge(
      length=case_width,
      width=41.2,
      height=39.55,
      r=1.5
    );

    translate([-wall, wall * 2, wall]) rounded_wedge(
        length=display_width,
        width=41.2,
        height=39.55,
        r=1.5
      );

    translate([0, 2, 5]) rotate([90 + 42, 0, 180]) display_cutout();
  }

  intersection() {
    translate([-wall, wall * 2, wall]) rounded_wedge(
        length=display_width,
        width=41.2,
        height=39.55,
        r=1.5
      );
    translate([-display_width / 2 - wall, display_length - 17, height * 1.7]) rotate([90, 0, 180]) leg(10);
  }
}
base();
