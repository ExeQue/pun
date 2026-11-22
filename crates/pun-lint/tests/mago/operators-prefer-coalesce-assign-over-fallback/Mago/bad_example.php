<?php

return $this->ids ?? ($this->ids = $this->readIds());
