class ChangeParentStationToNull < ActiveRecord::Migration[8.0]
  def change
    change_column_null :stops, :parent_station_id, true
  end
end
