class RenameHeadsign < ActiveRecord::Migration[8.0]
  def change
    rename_column :trips, :trip_headsign, :headsign
  end
end
