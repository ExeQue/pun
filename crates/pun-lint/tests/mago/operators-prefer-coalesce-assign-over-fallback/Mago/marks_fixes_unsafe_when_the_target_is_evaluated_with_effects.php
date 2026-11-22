<?php

$a = $this->ids ?? ($this->ids = []);
$b = $this->load()->ids ?? ($this->load()->ids = []);
$c = $map[next_key()] ?? ($map[next_key()] = []);
