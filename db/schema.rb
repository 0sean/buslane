# This file is auto-generated from the current state of the database. Instead
# of editing this file, please use the migrations feature of Active Record to
# incrementally modify your database, and then regenerate this schema definition.
#
# This file is the source Rails uses to define your schema when running `bin/rails
# db:schema:load`. When creating a new database, `bin/rails db:schema:load` tends to
# be faster and is potentially less error prone than running all of your
# migrations from scratch. Old migrations may fail to apply correctly if those
# migrations use external dependencies or application code.
#
# It's strongly recommended that you check this file into your version control system.

ActiveRecord::Schema[8.0].define(version: 2025_06_09_222119) do
  create_table "agencies", id: :string, force: :cascade do |t|
    t.string "name", null: false
    t.string "url"
    t.string "timezone", default: "Europe/London"
    t.string "lang", default: "EN"
    t.string "phone"
    t.string "noc", null: false
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
  end

  create_table "api_keys", force: :cascade do |t|
    t.string "name"
    t.text "token_digest"
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
    t.index ["token_digest"], name: "index_api_keys_on_token_digest", unique: true
  end

  create_table "calendar_dates", force: :cascade do |t|
    t.integer "service_id", null: false
    t.date "date", null: false
    t.integer "exception_type", null: false
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
    t.index ["service_id"], name: "index_calendar_dates_on_service_id"
  end

  create_table "frequencies", force: :cascade do |t|
    t.string "trip_id", null: false
    t.string "start_time", null: false
    t.string "end_time", null: false
    t.integer "headway_secs", null: false
    t.integer "exact_times", default: 0
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
    t.index ["trip_id"], name: "index_frequencies_on_trip_id"
  end

  create_table "routes", force: :cascade do |t|
    t.string "agency_id", null: false
    t.string "short_name"
    t.integer "long_name"
    t.integer "route_type", null: false
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
    t.index ["agency_id"], name: "index_routes_on_agency_id"
  end

  create_table "services", force: :cascade do |t|
    t.boolean "monday", default: false
    t.boolean "tuesday", default: false
    t.boolean "wednesday", default: false
    t.boolean "thursday", default: false
    t.boolean "friday", default: false
    t.boolean "saturday", default: false
    t.boolean "sunday", default: false
    t.date "start_date"
    t.date "end_date"
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
  end

  create_table "shapes", id: :string, force: :cascade do |t|
    t.string "pt_lat", null: false
    t.string "pt_lon", null: false
    t.integer "pt_sequence", null: false
    t.decimal "dist_traveled"
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
  end

  create_table "stop_times", force: :cascade do |t|
    t.string "stop_id", null: false
    t.string "trip_id", null: false
    t.string "arrival_time", null: false
    t.string "departure_time", null: false
    t.integer "stop_sequence"
    t.string "stop_headsign"
    t.integer "pickup_type", default: 0
    t.integer "drop_off_type", default: 0
    t.decimal "shape_dist_traveled"
    t.integer "timepoint", default: 1
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
    t.index ["stop_id"], name: "index_stop_times_on_stop_id"
    t.index ["trip_id"], name: "index_stop_times_on_trip_id"
  end

  create_table "stops", id: :string, force: :cascade do |t|
    t.string "code"
    t.string "name", null: false
    t.decimal "lat", null: false
    t.decimal "lon", null: false
    t.boolean "wheelchair_boarding", default: false
    t.integer "location_type", default: 0, null: false
    t.string "platform_code"
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
    t.string "parent_station_id"
    t.index ["parent_station_id"], name: "index_stops_on_parent_station_id"
  end

  create_table "trips", id: :string, force: :cascade do |t|
    t.integer "route_id", null: false
    t.integer "service_id", null: false
    t.string "headsign", default: ""
    t.integer "direction_id", null: false
    t.string "block_id", null: false
    t.boolean "wheelchair_accessible", default: false
    t.string "vehicle_journey_code", null: false
    t.datetime "created_at", null: false
    t.datetime "updated_at", null: false
    t.string "shape_id"
    t.index ["route_id"], name: "index_trips_on_route_id"
    t.index ["service_id"], name: "index_trips_on_service_id"
    t.index ["shape_id"], name: "index_trips_on_shape_id"
  end

  add_foreign_key "stops", "stops", column: "parent_station_id"
end
