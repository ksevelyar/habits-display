include <mixin.scad>;

display_pcb_length = 39;
display_pcb_width = display_width;

display_lcd_length = 37;
display_lcd_width = 30;

pcb_cutout_z = 0.7;
pcb_cutout_h = 3.1;
lcd_cutout_z = -0.1;
lcd_cutout_h = 1;

screw_inset = 2.1;
screw_top_d = 2;
screw_bot_d = 1.7;
screw_h = 3;

module display_cutout() {
  translate(
    [
      (case_width - display_pcb_width) / 2,
      (case_length - display_pcb_length) / 2,
      pcb_cutout_z,
    ]
  ) cube([display_pcb_width, display_pcb_length, pcb_cutout_h]);

  translate(
    [
      (case_width - display_lcd_width) / 2,
      (case_length - display_lcd_length) / 2,
      lcd_cutout_z,
    ]
  )
    rounded_box(size=[display_lcd_width, display_lcd_length, lcd_cutout_h], r=5);

  translate(
    [
      (case_width - display_pcb_width) / 2 + 3.5,
      (case_length - display_pcb_length) / 2 - 3.5,
      pcb_cutout_z,
    ]
  ) cube([display_lcd_width - 7, 2, pcb_cutout_h]);
}

module mcu_stopper() {
  difference() {
    translate([-case_width, case_length - 16.1 - wall*3 - esp32c3_length, wall - 0.1]) cube([case_width, 10, 6]);

    translate([-case_width, case_length - 16 - wall*3 - esp32c3_length, wall])
      rotate([30, 0, 0]) cube([case_width, 12, 6]);
  }
}

module base() {
  difference() {
    rounded_wedge(
      length=case_width,
      width=case_base_width,
      height=case_base_height,
      r=1.5
    );

    translate([-wall, wall * 2, wall]) rounded_wedge(
        length=display_width,
        width=case_base_width,
        height=case_base_height,
        r=1.5
      );

    translate([0, 2, 5]) rotate([90 + 42, 0, 180]) display_cutout();
  }

  translate([0, 2, 5]) rotate([90 + 42, 0, 180]) {
      translate([case_width - screw_inset - wall - 0.45, case_length - screw_inset - wall, 0.5]) cylinder(screw_h, d1=screw_top_d, d2=screw_bot_d);
      translate([wall + screw_inset + 0.45, case_length - screw_inset - wall, 0.5]) cylinder(screw_h, d1=screw_top_d, d2=screw_bot_d);
    }

  mcu_stopper();

  intersection() {
    translate([-wall, wall * 2, wall]) rounded_wedge(
        length=display_width,
        width=case_base_width,
        height=case_base_height,
        r=1.5
      );
    translate([-display_width / 2 - wall, display_length - 17, height * 1.65]) rotate([90, 0, 180]) leg(9);
  }
}
base();
