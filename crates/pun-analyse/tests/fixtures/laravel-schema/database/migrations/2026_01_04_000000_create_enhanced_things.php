<?php

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Tpetry\PostgresqlEnhanced\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('enhanced_things', function (Blueprint $table) {
            $table->string('label');
            $table->text('body')->nullable();
        });
    }
};
